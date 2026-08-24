use crate::{
    api::{
        agent::{agent_service_client::AgentServiceClient, PrepareArtifactRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, Artifact, ArtifactRequest,
            ArtifactSystem, ArtifactsRequest, ArtifactsResponse, GetArtifactAliasRequest,
        },
        context::context_service_server::{ContextService, ContextServiceServer},
    },
    artifact::system::get_system,
    cli::{Cli, Command},
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use http::uri::{InvalidUri, Uri};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, RefreshToken, TokenResponse, TokenUrl};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    net::{Ipv6Addr, SocketAddr},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    fs::{read, rename},
    io::AsyncWriteExt,
    sync::Mutex,
};
use tonic::{
    metadata::{Ascii, MetadataValue},
    transport::{Certificate, Channel, ClientTlsConfig, Server},
    Code::NotFound,
    Request, Response, Status,
};
use tracing::{debug, info};

/// Artifacts and lookup caches accumulated while a config runs, shared
/// between [`ConfigContext`] and the [`ConfigServer`] it starts.
#[derive(Clone)]
pub struct ConfigContextStore {
    artifact: HashMap<String, Artifact>,
    artifact_input_cache: HashMap<String, String>,
    variable: HashMap<String, String>,
}

/// Handle a Vorpal config binary uses to define and resolve artifacts for a
/// single build.
#[derive(Clone)]
pub struct ConfigContext {
    artifact: String,
    artifact_context: PathBuf,
    artifact_namespace: String,
    artifact_system: ArtifactSystem,
    artifact_unlock: bool,
    client_agent: AgentServiceClient<Channel>,
    client_artifact: ArtifactServiceClient<Channel>,
    port: u16,
    registry: String,
    store: ConfigContextStore,
}

/// gRPC [`ContextService`] implementation that serves a config's resolved
/// artifact store back to the `vorpal` CLI.
#[derive(Clone)]
pub struct ConfigServer {
    /// Artifact store served to callers over the `ContextService` API.
    pub store: ConfigContextStore,
}

/// Access and refresh token material for one issuer, as stored in the
/// on-disk credentials file.
#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentialsContent {
    /// Current access token used to authenticate registry requests.
    pub access_token: String,
    /// Audience parameter required by some `IdPs` (for example Auth0) when
    /// refreshing the access token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    /// `OAuth2` client ID the token was issued to.
    pub client_id: String,
    /// Lifetime of the access token, in seconds, from `issued_at`.
    pub expires_in: u64,
    /// Unix timestamp (seconds) at which the access token was issued.
    pub issued_at: u64,
    /// Refresh token used to obtain a new access token once it expires.
    pub refresh_token: String,
    /// `OAuth2` scopes granted to the access token.
    pub scopes: Vec<String>,
}

/// On-disk contents of the Vorpal credentials file: per-issuer token
/// material plus the issuer each registry authenticates against.
#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentials {
    /// Token material for each known issuer, keyed by issuer URL.
    pub issuer: BTreeMap<String, VorpalCredentialsContent>,
    /// Issuer URL used by each registry, keyed by registry address.
    pub registry: BTreeMap<String, String>,
}

/// Default namespace when none is specified in an artifact alias.
pub const DEFAULT_NAMESPACE: &str = "library";

/// Default tag when none is specified in an artifact alias.
pub const DEFAULT_TAG: &str = "latest";

/// Parsed components of an artifact alias.
///
/// Alias format: `[<namespace>/]<name>[:<tag>]`
/// - namespace defaults to [`DEFAULT_NAMESPACE`] when omitted
/// - tag defaults to [`DEFAULT_TAG`] when omitted
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactAlias {
    /// Artifact name component of the alias.
    pub name: String,
    /// Namespace component of the alias, defaulted to [`DEFAULT_NAMESPACE`]
    /// when the alias omits it.
    pub namespace: String,
    /// Tag component of the alias, defaulted to [`DEFAULT_TAG`] when the
    /// alias omits it.
    pub tag: String,
}

/// Returns `true` if `s` is non-empty and every character is in the allowed set
/// for alias components: alphanumeric (`a-z`, `A-Z`, `0-9`), hyphens (`-`),
/// dots (`.`), underscores (`_`), and plus signs (`+`).
fn is_valid_component(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '+'))
}

/// Parses an artifact alias string into its components.
///
/// Format: `[<namespace>/]<name>[:<tag>]`
/// - namespace is optional (defaults to [`DEFAULT_NAMESPACE`])
/// - tag is optional (defaults to [`DEFAULT_TAG`])
/// - name is required
///
/// Each component (name, namespace, tag) may only contain alphanumeric
/// characters, hyphens, dots, underscores, and plus signs.
///
/// This mirrors the Go implementation in `sdk/go/pkg/config/context.go`.
///
/// # Errors
///
/// Returns an error if `alias` is empty, longer than 255 characters, has an
/// empty tag or namespace segment, has more than one `/` separator, has no
/// name, or has a name, namespace, or tag containing characters outside the
/// allowed set.
pub fn parse_artifact_alias(alias: &str) -> Result<ArtifactAlias> {
    if alias.is_empty() {
        bail!("alias cannot be empty");
    }

    if alias.len() > 255 {
        bail!("alias too long (max 255 characters)");
    }

    // Step 1: Extract tag (split on rightmost ':')
    let (base, tag) = match alias.rsplit_once(':') {
        Some((_, "")) => bail!("tag cannot be empty"),
        Some((b, t)) => (b, t.to_string()),
        None => (alias, String::new()),
    };

    // Step 2: Extract namespace/name (split on '/')
    let (namespace, name) = match base.split_once('/') {
        Some(("", _)) => bail!("namespace cannot be empty"),
        Some((_ns, rest)) if rest.contains('/') => {
            bail!("invalid format: too many path separators")
        }
        Some((ns, name)) => (ns.to_string(), name.to_string()),
        None => (String::new(), base.to_string()),
    };

    if name.is_empty() {
        bail!("name is required");
    }

    // Step 3: Validate component characters
    if !is_valid_component(&name) {
        bail!("name contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)");
    }

    if !namespace.is_empty() && !is_valid_component(&namespace) {
        bail!("namespace contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)");
    }

    if !tag.is_empty() && !is_valid_component(&tag) {
        bail!("tag contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)");
    }

    // Step 4: Apply defaults
    let tag = if tag.is_empty() {
        DEFAULT_TAG.to_string()
    } else {
        tag
    };

    let namespace = if namespace.is_empty() {
        DEFAULT_NAMESPACE.to_string()
    } else {
        namespace
    };

    Ok(ArtifactAlias {
        name,
        namespace,
        tag,
    })
}

impl ConfigServer {
    /// Creates a server that serves the given artifact store.
    #[must_use]
    pub fn new(store: ConfigContextStore) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl ContextService for ConfigServer {
    async fn get_artifact(
        &self,
        request: Request<ArtifactRequest>,
    ) -> Result<Response<Artifact>, Status> {
        let request = request.into_inner();

        if request.digest.is_empty() {
            return Err(tonic::Status::invalid_argument("'digest' is required"));
        }

        let artifact = self
            .store
            .artifact
            .get(request.digest.as_str())
            .ok_or_else(|| tonic::Status::not_found("artifact not found"))?;

        Ok(Response::new(artifact.clone()))
    }

    async fn get_artifacts(
        &self,
        _: tonic::Request<ArtifactsRequest>,
    ) -> Result<tonic::Response<ArtifactsResponse>, tonic::Status> {
        let mut digests: Vec<String> = self.store.artifact.keys().cloned().collect();
        digests.sort();

        let response = ArtifactsResponse { digests };

        Ok(Response::new(response))
    }
}

/// Parses CLI arguments and connects to the agent and registry services to
/// build a [`ConfigContext`] for the current run.
///
/// # Errors
///
/// Returns an error if connecting to the agent or registry service fails, or
/// if `artifact_system` does not name a supported system.
pub async fn get_context() -> Result<ConfigContext> {
    let args = Cli::parse();

    match args.command {
        Command::Start {
            agent,
            artifact,
            artifact_context,
            artifact_namespace,
            artifact_system,
            artifact_unlock,
            artifact_variable,
            port,
            registry,
        } => {
            let client_agent_channel = build_channel(&agent).await?;
            let client_registry_channel = build_channel(&registry).await?;

            let client_agent = AgentServiceClient::new(client_agent_channel);
            let client_artifact = ArtifactServiceClient::new(client_registry_channel);

            Ok(ConfigContext::new(
                artifact,
                PathBuf::from(artifact_context),
                artifact_namespace,
                artifact_system,
                artifact_unlock,
                artifact_variable,
                client_agent,
                client_artifact,
                port,
                registry,
            )?)
        }
    }
}

impl ConfigContext {
    /// Builds a context from the resolved `start` subcommand flags and
    /// connected service clients.
    ///
    /// # Errors
    ///
    /// Returns an error if `artifact_system` does not name a supported
    /// system.
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor takes the ten flags of the `start` subcommand one-to-one"
    )]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public SDK API; changing the signature is a breaking change"
    )]
    pub fn new(
        artifact: String,
        artifact_context: PathBuf,
        artifact_namespace: String,
        artifact_system: String,
        artifact_unlock: bool,
        artifact_variable: Vec<String>,
        client_agent: AgentServiceClient<Channel>,
        client_artifact: ArtifactServiceClient<Channel>,
        port: u16,
        registry: String,
    ) -> Result<Self> {
        Ok(Self {
            artifact,
            artifact_context,
            client_agent,
            client_artifact,
            artifact_namespace,
            port,
            registry,
            store: ConfigContextStore {
                artifact: HashMap::new(),
                artifact_input_cache: HashMap::new(),
                variable: artifact_variable
                    .iter()
                    .map(|v| {
                        let mut parts = v.split('=');
                        let name = parts.next().unwrap_or_default();
                        let value = parts.next().unwrap_or_default();
                        (name.to_string(), value.to_string())
                    })
                    .collect(),
            },
            artifact_system: get_system(&artifact_system)?,
            artifact_unlock,
        })
    }

    /// Sends `artifact` to the agent service to be prepared (locked and
    /// hashed), caching the result so identical artifacts are only prepared
    /// once. Returns the digest of the prepared artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if `artifact` has an empty name, no steps, no
    /// systems, or a target system not listed in its systems; if the
    /// artifact cannot be serialized; if the agent request fails to
    /// authenticate or the agent service returns an error other than "not
    /// found"; or if the agent service does not return a prepared artifact
    /// and its digest.
    pub async fn add_artifact(&mut self, artifact: Artifact) -> Result<String> {
        if artifact.name.is_empty() {
            bail!("name cannot be empty");
        }

        if artifact.steps.is_empty() {
            bail!("steps cannot be empty");
        }

        if artifact.systems.is_empty() {
            bail!("systems cannot be empty");
        }

        // Validate target is in systems list
        if !artifact.systems.contains(&artifact.target) {
            bail!(
                "artifact '{}' does not support system '{:?}' (supported: {:?})",
                artifact.name,
                ArtifactSystem::try_from(artifact.target).unwrap_or(ArtifactSystem::UnknownSystem),
                artifact
                    .systems
                    .iter()
                    .filter_map(|&s| ArtifactSystem::try_from(s).ok())
                    .collect::<Vec<_>>()
            );
        }

        // Send raw sources to agent - agent will handle all lockfile operations
        let artifact_json =
            serde_json::to_vec(&artifact).context("failed to serialize artifact to JSON")?;

        let input_digest = digest(artifact_json);

        if self.store.artifact.contains_key(&input_digest) {
            return Ok(input_digest);
        }

        if let Some(output_digest) = self.store.artifact_input_cache.get(&input_digest) {
            if self.store.artifact.contains_key(output_digest) {
                return Ok(output_digest.clone());
            }
        }

        // TODO: make this run in parallel

        // The request owns the artifact once sent; only the name is kept for
        // progress output.
        let artifact_name = artifact.name.clone();

        let request = PrepareArtifactRequest {
            artifact: Some(artifact),
            artifact_context: self.artifact_context.display().to_string(),
            artifact_namespace: self.artifact_namespace.clone(),
            artifact_unlock: self.artifact_unlock,
            registry: self.registry.clone(),
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        let response = self
            .client_agent
            .prepare_artifact(request)
            .await
            .context("failed to prepare artifact")?;

        let mut response = response.into_inner();
        let mut response_artifact = None;
        let mut response_artifact_digest = None;

        loop {
            match response.message().await {
                Ok(Some(message)) => {
                    if let Some(artifact_output) = message.artifact_output {
                        if self.port == 0 {
                            info!("{artifact_name} |> {artifact_output}");
                        } else {
                            emit(format!("{artifact_name} |> {artifact_output}"));
                        }
                    }

                    response_artifact = message.artifact;
                    response_artifact_digest = message.artifact_digest;
                }
                Ok(None) => break,
                Err(status) => {
                    if status.code() != NotFound {
                        bail!("{}", status.message());
                    }

                    break;
                }
            }
        }

        let Some(artifact) = response_artifact else {
            bail!("artifact not returned from agent service");
        };

        let Some(artifact_digest) = response_artifact_digest else {
            bail!("artifact digest not returned from agent service");
        };

        self.store
            .artifact_input_cache
            .insert(input_digest, artifact_digest.clone());

        self.store
            .artifact
            .insert(artifact_digest.clone(), artifact);

        Ok(artifact_digest)
    }

    /// Fetches an artifact and its transitive dependencies from the registry
    /// into the local store, in the context's own namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the registry request fails to authenticate, the
    /// artifact is not found in the registry, or the registry returns an
    /// error other than "not found".
    pub async fn fetch_artifact(&mut self, digest: &str) -> Result<String> {
        Self::fetch_artifact_in_namespace(
            &mut self.store,
            &mut self.client_artifact,
            &self.registry,
            digest,
            &self.artifact_namespace,
        )
        .await
    }

    /// Takes the store and client as separate borrows so callers can pass a
    /// namespace borrowed from another field of `self` while recursing.
    async fn fetch_artifact_in_namespace(
        store: &mut ConfigContextStore,
        client_artifact: &mut ArtifactServiceClient<Channel>,
        registry: &str,
        digest: &str,
        namespace: &str,
    ) -> Result<String> {
        if store.artifact.contains_key(digest) {
            return Ok(digest.to_string());
        }

        // TODO: look in lockfile for artifact version

        let request = ArtifactRequest {
            digest: digest.to_string(),
            namespace: namespace.to_string(),
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        match client_artifact.get_artifact(request).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("artifact service error: {status:?}");
                }

                bail!("artifact not found: {digest}");
            }

            Ok(response) => {
                let artifact = response.into_inner();

                // Dependencies are fetched before the artifact is stored so the
                // artifact can be moved into the store afterwards. Digests are
                // content-addressed, so the dependency graph cannot cycle back
                // to `digest` and the store's final contents are unchanged.
                for step in &artifact.steps {
                    for dep in &step.artifacts {
                        Box::pin(Self::fetch_artifact_in_namespace(
                            store,
                            client_artifact,
                            registry,
                            dep,
                            namespace,
                        ))
                        .await?;
                    }
                }

                store.artifact.insert(digest.to_string(), artifact);

                Ok(digest.to_string())
            }
        }
    }

    /// Resolves an artifact alias (`[<namespace>/]<name>[:<tag>]`) to a
    /// digest via the registry, then fetches that artifact and its
    /// transitive dependencies into the local store.
    ///
    /// # Errors
    ///
    /// Returns an error if `alias` fails to parse, the registry request
    /// fails to authenticate, the alias is not found in the registry, the
    /// registry returns an empty digest or another error, or fetching the
    /// resolved artifact fails.
    pub async fn fetch_artifact_alias(&mut self, alias: &str) -> Result<String> {
        let alias_parsed = parse_artifact_alias(alias)?;

        let request = GetArtifactAliasRequest {
            system: self.artifact_system.into(),
            name: alias_parsed.name,
            namespace: alias_parsed.namespace.clone(),
            tag: alias_parsed.tag,
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        let response = self
            .client_artifact
            .get_artifact_alias(request)
            .await
            .map_err(|status| {
                if status.code() == NotFound {
                    anyhow!("alias not found in registry: {alias}")
                } else {
                    anyhow!("registry error: {status:?}")
                }
            })?;

        let digest = response.into_inner().digest;

        if digest.is_empty() {
            bail!("registry returned empty digest for alias: {alias}");
        }

        if self.store.artifact.contains_key(&digest) {
            return Ok(digest);
        }

        Self::fetch_artifact_in_namespace(
            &mut self.store,
            &mut self.client_artifact,
            &self.registry,
            &digest,
            &alias_parsed.namespace,
        )
        .await?;

        Ok(digest)
    }

    /// Returns all artifacts resolved into the local store so far.
    pub fn get_artifact_store(&self) -> &HashMap<String, Artifact> {
        &self.store.artifact
    }

    /// Returns the artifact resolved into the local store under `digest`, if
    /// any.
    pub fn get_artifact(&self, digest: &str) -> Option<Artifact> {
        self.store.artifact.get(digest).cloned()
    }

    /// Returns the path to the artifact's source context directory.
    pub fn get_artifact_context_path(&self) -> &PathBuf {
        &self.artifact_context
    }

    /// Returns the name of the artifact this context is building.
    pub fn get_artifact_name(&self) -> &str {
        self.artifact.as_str()
    }

    /// Returns the namespace the artifact belongs to.
    pub fn get_artifact_namespace(&self) -> &str {
        self.artifact_namespace.as_str()
    }

    /// Returns the target system this context is building for.
    pub fn get_system(&self) -> ArtifactSystem {
        self.artifact_system
    }

    /// Returns the value of the `key=value` build variable named `name`, if
    /// it was set on the command line.
    pub fn get_variable(&self, name: &str) -> Option<String> {
        self.store.variable.get(name).cloned()
    }

    /// Runs the `ContextService` gRPC server, serving this context's
    /// resolved artifact store to the `vorpal` CLI until the server exits.
    /// Consumes the context: the store moves into the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to serve on the configured port.
    pub async fn run(self) -> Result<()> {
        let service = ContextServiceServer::new(ConfigServer::new(self.store));

        let service_addr_str = format!("[::]:{}", self.port);
        let service_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, self.port));

        emit(format!("context service: {service_addr_str}"));

        Server::builder()
            .add_service(service)
            .serve(service_addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {e}"))
    }
}

/// Writes a line to standard output.
///
/// The SDK's single sanctioned terminal output path: config binaries report
/// build progress on stdout, distinct from the `tracing` output used when the
/// binary runs as a service.
#[expect(
    clippy::print_stdout,
    reason = "the SDK's single sanctioned terminal output path; config binaries report progress on stdout"
)]
fn emit(line: impl std::fmt::Display) {
    println!("{line}");
}

/// Returns the root directory Vorpal stores its runtime state under.
#[must_use]
pub fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
}

/// Returns the directory Vorpal stores key material under.
#[must_use]
pub fn get_root_key_dir_path() -> PathBuf {
    get_root_dir_path().join("key")
}

/// Returns the path to the CA certificate used to verify TLS connections to
/// Vorpal services.
#[must_use]
pub fn get_key_ca_path() -> PathBuf {
    get_root_key_dir_path().join("ca").with_extension("pem")
}

/// Returns the path to the on-disk [`VorpalCredentials`] file.
#[must_use]
pub fn get_key_credentials_path() -> PathBuf {
    get_root_key_dir_path()
        .join("credentials")
        .with_extension("json")
}

async fn get_client_tls_config(uri: &str) -> Result<Option<ClientTlsConfig>> {
    if uri.starts_with("http://") || uri.starts_with("unix://") {
        return Ok(None);
    }

    let ca_pem_path = get_key_ca_path();

    let mut client_tls_config = ClientTlsConfig::new().with_native_roots();

    if ca_pem_path.exists() {
        let ca_pem = read(&ca_pem_path)
            .await
            .with_context(|| format!("failed to read CA certificate: {}", ca_pem_path.display()))?;

        client_tls_config = client_tls_config.ca_certificate(Certificate::from_pem(ca_pem));
    }

    Ok(Some(client_tls_config))
}

/// Connects to a Vorpal service at `uri`, which may be an `http://`,
/// `https://`, or `unix://` address.
///
/// # Errors
///
/// Returns an error if `uri` does not start with `http://`, `https://`, or
/// `unix://`; if `uri` fails to parse; if the TLS configuration for an
/// `https://` address cannot be built; or if connecting to the service fails.
pub async fn build_channel(uri: &str) -> Result<Channel> {
    // Handle Unix domain socket connections
    if let Some(socket_path) = uri.strip_prefix("unix://") {
        let socket_path = socket_path.to_string();

        // Dummy URI required by tonic's channel builder; ignored when using a custom connector.
        // Uses connect_with_connector_lazy so the channel is created immediately and the
        // actual connection is deferred until the first RPC call, avoiding startup races
        // when the client is created before the server socket is ready.
        let channel = Channel::from_static("http://[::]:50051").connect_with_connector_lazy(
            tower::service_fn(move |_: tonic::transport::Uri| {
                let path = socket_path.clone();
                async move {
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(
                        tokio::net::UnixStream::connect(path).await?,
                    ))
                }
            }),
        );

        return Ok(channel);
    }

    if !uri.starts_with("http://") && !uri.starts_with("https://") {
        bail!("URI must start with http://, https://, or unix://: {uri}");
    }

    let parsed_uri = uri
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow!("invalid URI: {e}"))?;

    let tls_config = get_client_tls_config(uri).await?;

    let mut endpoint = Channel::builder(parsed_uri);

    if let Some(tls) = tls_config {
        endpoint = endpoint.tls_config(tls)?;
    }

    endpoint
        .connect()
        .await
        .with_context(|| format!("failed to connect to {uri}"))
}

/// Bounds a single refresh exchange so a hung IdP stalls only that exchange
/// rather than every caller queued behind [`CREDENTIALS_REFRESH`].
const REFRESH_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A completed refresh exchange: `(access_token, expires_in, issued_at,
/// rotated_refresh_token)`. `rotated_refresh_token` is `Some(new)` when the
/// IdP rotated the refresh token (Zitadel default), `None` when it did not
/// (caller keeps the existing refresh token).
type RefreshedToken = (String, u64, u64, Option<String>);

/// Why a refresh exchange failed, discriminated by whether the refresh token
/// had already left this process.
///
/// The distinction is what makes the failure safe to act on. A token that
/// reached the IdP may have been consumed and rotated even when the outcome
/// never came back, so it must never be sent a second time. A token that
/// never left is untouched: the fault was local (bad URL, unreachable
/// discovery endpoint, malformed document), and retrying it once that clears
/// is correct — latching on those instead bricks every authenticated call in
/// a long-lived process for a transient DNS failure.
enum RefreshFailure {
    /// The exchange was abandoned before the token was put on the wire.
    NotSent(anyhow::Error),

    /// The token-endpoint request was issued. The IdP may have consumed the
    /// token whatever came back — including nothing at all.
    Sent(anyhow::Error),
}

impl From<RefreshFailure> for anyhow::Error {
    fn from(failure: RefreshFailure) -> Self {
        match failure {
            RefreshFailure::NotSent(err) | RefreshFailure::Sent(err) => err,
        }
    }
}

/// Parses an OIDC URL into its `scheme://host:port` origin, rejecting any
/// destination a refresh token must not be sent to.
///
/// Plaintext HTTP is refused except on loopback, where there is no network to
/// eavesdrop and local IdP fixtures live. The origin it returns is what pins
/// the token endpoint — named by a remote discovery document — to the issuer
/// the user actually logged in to.
fn credential_egress_origin(raw: &str) -> Result<String> {
    let url = reqwest::Url::parse(raw).with_context(|| format!("invalid OIDC URL: {}", raw))?;

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("OIDC URL has no host: {}", raw))?;

    let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");

    match url.scheme() {
        "https" => {}
        "http" if is_loopback => {}
        scheme => bail!(
            "refusing to send a refresh token over {} to {}: the OIDC issuer must be https",
            scheme,
            host
        ),
    }

    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("OIDC URL has no port: {}", raw))?;

    Ok(format!("{}://{}:{}", url.scheme(), host, port))
}

/// Refreshes an expired access token using the refresh token.
///
/// Returns `(access_token, expires_in, issued_at, rotated_refresh_token)`.
/// `rotated_refresh_token` is `Some(new)` when the `IdP` rotated the refresh
/// token (Zitadel default), `None` when it did not (caller should keep the
/// existing refresh token).
///
/// Every error is classified as [`RefreshFailure::NotSent`] or
/// [`RefreshFailure::Sent`] at the point it arises: everything up to and
/// including the discovery round trip happens before the token is on the
/// wire, and everything from the token-endpoint request onward happens after
/// the IdP could have consumed it.
async fn refresh_access_token(
    audience: Option<&str>,
    client_id: &str,
    issuer: &str,
    refresh_token: &str,
    timeout: std::time::Duration,
) -> std::result::Result<RefreshedToken, RefreshFailure> {
    // Redirects are refused rather than followed: a 307 from a tampered or
    // compromised token endpoint would otherwise replay the credential-bearing
    // POST to a host of the redirector's choosing. The login flow in
    // `cli/src/command.rs` pins the same policy.
    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .map_err(|e| RefreshFailure::NotSent(e.into()))?;

    let issuer_origin = credential_egress_origin(issuer).map_err(RefreshFailure::NotSent)?;

    // Discover token endpoint
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let doc: serde_json::Value = http_client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| RefreshFailure::NotSent(e.into()))?
        .json()
        .await
        .map_err(|e| RefreshFailure::NotSent(e.into()))?;

    let token_endpoint = doc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            RefreshFailure::NotSent(anyhow!("missing token_endpoint in OIDC discovery"))
        })?;

    let token_endpoint_origin =
        credential_egress_origin(token_endpoint).map_err(RefreshFailure::NotSent)?;

    // Contract, not incidental: the discovery document's token_endpoint must
    // share the issuer's scheme://host:port. This is deliberate — it closes
    // an egress-redirection hazard in which a tampered or compromised
    // discovery document steers the credential-bearing POST to a host of its
    // own choosing, and it is the sibling of the `redirect::Policy::none()`
    // set above: an origin pin without a redirect ban is not a pin. A
    // legitimate multi-host IdP deployment must be reconciled with this
    // constraint (e.g. by fronting the token endpoint on the issuer's own
    // origin) rather than this check being relaxed or bypassed.
    if token_endpoint_origin != issuer_origin {
        return Err(RefreshFailure::NotSent(anyhow!(
            "OIDC token_endpoint origin {} does not match issuer origin {}",
            token_endpoint_origin,
            issuer_origin
        )));
    }

    // Create OAuth2 client
    let auth_uri = AuthUrl::new(issuer.to_string())
        .map_err(|e| RefreshFailure::NotSent(anyhow!("invalid issuer URL: {}", e)))?;
    let token_uri = TokenUrl::new(token_endpoint.to_string())
        .map_err(|e| RefreshFailure::NotSent(anyhow!("invalid token_endpoint URL: {}", e)))?;

    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth_uri)
        .set_token_uri(token_uri);

    // Exchange refresh token
    let refresh_token_obj = RefreshToken::new(refresh_token.to_string());
    let mut request = client.exchange_refresh_token(&refresh_token_obj);

    // Only add audience if provided (Auth0 requires it, others may not)
    if let Some(aud) = audience {
        request = request.add_extra_param("audience", aud);
    }

    // From here on the token is on the wire: a transport error, a timeout and
    // a rejection are indistinguishable from the IdP having consumed it.
    let token_result = request
        .request_async(&http_client)
        .await
        .map_err(|e| RefreshFailure::Sent(anyhow!("OAuth refresh-token exchange failed: {}", e)))?;

    let new_access_token = token_result.access_token().secret().clone();
    let new_expires_in = token_result.expires_in().map_or(3600, |d| d.as_secs());
    let new_refresh_token =
        normalize_rotated_refresh_token(token_result.refresh_token().map(|t| t.secret().clone()));

    let issued_at = system_now().map_err(RefreshFailure::Sent)?;

    Ok((
        new_access_token,
        new_expires_in,
        issued_at,
        new_refresh_token,
    ))
}

/// Normalizes the `refresh_token` field from an OIDC token-refresh response.
///
/// Some `IdPs` send `"refresh_token": ""` in the response body, which the
/// `oauth2` crate may surface as `Some(RefreshToken(""))`. Treat that as
/// "not rotated" so callers do not overwrite the stored refresh token with
/// an empty string. Mirrors the Go and TypeScript SDK behavior.
fn normalize_rotated_refresh_token(raw: Option<String>) -> Option<String> {
    raw.filter(|s| !s.is_empty())
}

/// Applies the result of a token-refresh response to an existing credentials
/// record. The rotated refresh token, when present, replaces the stored one;
/// when absent the existing refresh token is left untouched. Other fields
/// (`audience`, `client_id`, `scopes`) are never modified here.
fn apply_token_refresh(
    creds: &mut VorpalCredentialsContent,
    access_token: String,
    expires_in: u64,
    issued_at: u64,
    rotated_refresh_token: Option<String>,
) {
    creds.access_token = access_token;
    creds.expires_in = expires_in;
    creds.issued_at = issued_at;
    if let Some(new) = rotated_refresh_token {
        creds.refresh_token = new;
    }
}

/// Counter for unique temp-file names in `write_credentials_secure`. Paired
/// with the process id; `create_new` below is what actually makes the write
/// fail-closed rather than following a pre-existing path, so the name only
/// needs to be unique, not unpredictable.
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// How many candidate temp-file names [`write_credentials_secure_with_names`]
/// tries before giving up — see its doc comment for why a collision happens
/// at all.
const TEMP_FILE_MAX_ATTEMPTS: usize = 8;

/// The temp-file names a credentials write at `path` draws from, in order:
/// `<file>.<pid>.<counter>.tmp`, endlessly, one fresh counter value per draw.
///
/// Unbounded on purpose: [`write_credentials_secure_with_names`] owns the
/// bound, so this iterator can never be the thing that cuts a retry short.
/// `fetch_add` is what makes successive draws distinct; a load without the
/// add hands the retry the same name it just collided on.
fn temp_file_candidate_names(path: &Path) -> impl Iterator<Item = String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("credentials")
        .to_string();
    let pid = std::process::id();

    std::iter::repeat_with(move || {
        format!(
            "{}.{}.{}.tmp",
            file_name,
            pid,
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    })
}

/// Unlinks the temp file at `path` when dropped, unless [`disarm`] was
/// called first. Owns `write_credentials_secure`'s temp file from the moment
/// that file is created through to a committed `rename`, so a cancelled
/// task, a dropped `JoinSet`, or a panic still removes a file that may hold
/// a live plaintext refresh token (VPL-183 AB-3).
///
/// The guard is constructed in the same blocking unit that creates the file,
/// never around an `.await`: arming earlier would unlink a path this call
/// never created, and arming later would leave the created file unowned for
/// as long as the task can be cancelled.
///
/// `Drop` cannot `.await`, so this uses the blocking `std::fs::remove_file`
/// rather than `tokio::fs::remove_file`. This is the one place in this file
/// where a synchronous filesystem call is correct; do not "fix" it back to
/// an async call.
///
/// Honest boundary: `Drop` does not run on `exit(1)` (e.g.
/// `cli/src/command/build.rs`) or `SIGKILL`. Those exits are not covered by
/// any destructor-based guard and are an accepted residual risk, not a gap
/// in this one.
///
/// [`disarm`]: TempFileGuard::disarm
struct TempFileGuard {
    armed: bool,
    path: PathBuf,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { armed: true, path }
    }

    /// The temp file this guard owns, and the only path the committing
    /// `rename` may name.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Marks the temp file as committed (renamed onto its destination) so
    /// `Drop` leaves it alone.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            // Best effort — `Drop` cannot propagate — but never silent: a
            // discarded error here is the difference between a temp file
            // holding a live refresh token being cleaned up and being
            // leaked, and nothing else reports which happened.
            if let Err(err) = std::fs::remove_file(&self.path) {
                debug!(
                    "failed to remove temp credentials file {}: {}",
                    self.path.display(),
                    err
                );
            }
        }
    }
}

/// Writes credential bytes to `path` atomically: a temp file in the same
/// directory (required for `rename` to be atomic — it is only atomic within
/// a filesystem) is created, written, `fsync`'d and renamed onto `path`.
///
/// The temp file is created with `create_new` (`O_EXCL`) and mode 0o600, and
/// `rename(2)` replaces the destination inode with the source inode — so the
/// destination's mode after the rename is the temp file's mode, not the old
/// destination's. The 0o600 enforcement therefore lives on the temp file,
/// not on `path`; both Rust call sites for `credentials.json` (login at
/// `cli/src/command.rs` and refresh here) must preserve that, or the file
/// can end up world-readable via the umask-masked mode of a naive
/// `File::create` (see the local archive-store writer's temp-file creation
/// for the shape that must NOT be copied here).
///
/// [`TempFileGuard`] owns the temp file from creation onward — success
/// disarms it after `rename` commits; any early return, cancellation, or
/// panic leaves it armed and `Drop` unlinks the file.
///
/// A `create_new` collision (`ErrorKind::AlreadyExists`) is retried against a
/// fresh candidate name rather than failing closed on the first stale
/// leftover (VPL-283 C-4): see [`write_credentials_secure_with_names`].
async fn write_credentials_secure(path: &Path, bytes: &[u8]) -> Result<()> {
    write_credentials_secure_with_names(path, bytes, temp_file_candidate_names(path)).await
}

/// Core of [`write_credentials_secure`], taking the candidate temp-file names
/// as a parameter so a collision on the first one is testable without racing
/// the real `TEMP_FILE_COUNTER` static under Rust's parallel test harness.
/// Deliberately private and not configurable from any production entry point
/// (env var, global override) — same discipline as `client_auth_header_at`,
/// see its doc comment. Each name must be a bare file name: it is joined onto
/// `path`'s parent directory, and `Path::join` would let an absolute or
/// `..`-bearing component relocate the temp file out of the key directory,
/// where the committing `rename` can no longer be atomic.
///
/// `TEMP_FILE_COUNTER` resets to 0 on every process start, so a leaked temp
/// file from a prior invocation that reused this process's PID and counter
/// value collides on `create_new`; without a retry that one stale leftover
/// fails the write closed and burns the credential (VPL-283 AB-283-4). At
/// most [`TEMP_FILE_MAX_ATTEMPTS`] names are tried and exhaustion returns
/// `Err`. The bound lives here as a ceiling no caller can raise or remove; a
/// caller may still offer fewer names than that, and both test callers do, in
/// which case exhaustion is reported against the number actually offered.
/// This is defense against a stale file, not against an adversarial loop (a
/// same-UID adversary that could occupy every name already has the token,
/// VPL-283 threat model A2).
///
/// Every attempt uses `create_new` (`O_EXCL`) and `.mode(0o600)`: never falls
/// back to a non-exclusive open (that would follow a pre-existing path
/// instead of refusing it) and never unlinks or truncates a name that is
/// already taken (that name may belong to a concurrently-writing same-UID
/// process). Both are the fail-open directions this retry must not
/// reintroduce (VPL-283 C-4).
async fn write_credentials_secure_with_names(
    path: &Path,
    bytes: &[u8],
    candidate_names: impl Iterator<Item = String> + Send,
) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "credentials path has no parent directory: {}",
            path.display()
        )
    })?;

    let mut candidate_names = candidate_names.take(TEMP_FILE_MAX_ATTEMPTS);
    let mut attempts = 0usize;

    // Create the file and arm its guard in one blocking unit. A
    // `spawn_blocking` task runs to completion even when the future awaiting
    // it is dropped, so the guard and the file it owns come into existence
    // together and are dropped together; splitting them around an `.await`
    // leaves a cancellation window in which the open still lands but the
    // guard is already gone, and the temp file outlives it (VPL-183 AB-9).
    // Each retry attempt is its own `spawn_blocking` unit so this property
    // holds per attempt, not just for the first one.
    let (mut guard, file) = loop {
        let Some(name) = candidate_names.next() else {
            // No name was ever drawn, so nothing collided: reporting this as
            // exhaustion would claim a collision that never happened, which
            // is the distinction the exhaustion message exists to make.
            if attempts == 0 {
                bail!(
                    "no candidate temp-file name was offered for writing {}",
                    path.display()
                );
            }

            bail!(
                "every one of {} candidate temp-file names was already taken writing {}",
                attempts,
                path.display()
            );
        };

        attempts += 1;

        let open_path = parent.join(name);

        let opened: std::io::Result<(TempFileGuard, std::fs::File)> =
            tokio::task::spawn_blocking(move || {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&open_path)
                    .map(|file| (TempFileGuard::new(open_path), file))
            })
            .await?;

        match opened {
            Ok(opened) => break opened,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    };

    let mut file = tokio::fs::File::from_std(file);

    file.write_all(bytes).await?;
    // fsync, not flush: flush is a userspace buffer flush, and a rename
    // ordered before the data reaches disk can leave a zero-length
    // credentials.json after a crash.
    file.sync_all().await?;

    rename(guard.path(), path).await?;

    guard.disarm();

    Ok(())
}

/// Performs the OAuth refresh-token exchange. Injected as a [`TokenRefresher`]
/// so tests can count and control exchanges without real network I/O.
///
/// Contract: `refresh` runs while [`CREDENTIALS_REFRESH`] is locked and must
/// not call back into `client_auth_header` or `client_auth_header_at` — that
/// lock is a non-reentrant `tokio::sync::Mutex`, and a re-entrant call
/// deadlocks every authenticated RPC in the process.
#[tonic::async_trait]
trait TokenRefresher: Send + Sync {
    async fn refresh(
        &self,
        audience: Option<&str>,
        client_id: &str,
        issuer: &str,
        refresh_token: &str,
    ) -> std::result::Result<RefreshedToken, RefreshFailure>;
}

struct LiveTokenRefresher;

#[tonic::async_trait]
impl TokenRefresher for LiveTokenRefresher {
    async fn refresh(
        &self,
        audience: Option<&str>,
        client_id: &str,
        issuer: &str,
        refresh_token: &str,
    ) -> std::result::Result<RefreshedToken, RefreshFailure> {
        refresh_access_token(
            audience,
            client_id,
            issuer,
            refresh_token,
            REFRESH_HTTP_TIMEOUT,
        )
        .await
    }
}

/// Refresh-token values this process has already put on the wire without
/// durably committing a replacement, keyed by a SHA-256 digest of the token
/// value — never the plaintext, and never logged.
///
/// A failed exchange, and a successful one whose result never reaches disk,
/// both leave `credentials.json` byte-identical. Re-reading that file cannot
/// tell either apart from a token that has simply never been tried, so every
/// waiter re-decides "needs refresh" and replays the same one-time token
/// against the IdP (VPL-183 AB-1). Serializing the callers spaces that replay
/// out in time; only remembering the exchange's outcome prevents it.
///
/// Being spent is terminal for that token *value*, not a backoff: a
/// time-based retry would replay the same already-consumed token once the
/// timer expired, which is the same hazard with a delay. Keying on the value
/// rather than the issuer is what keeps that from spreading — a legitimately
/// rotated token has a different digest and is unaffected by an older
/// value's terminal failure.
struct RefreshState {
    spent: BTreeSet<String>,
}

impl RefreshState {
    const fn new() -> Self {
        Self {
            spent: BTreeSet::new(),
        }
    }
}

/// Serializes credential read, refresh-decision, refresh exchange and file
/// write within this process, and guards the spent-token memo those steps
/// consult. Held for the whole critical section, not just the write, so a
/// waiter re-reads the winner's post-refresh state instead of retrying a
/// decision it made against a stale snapshot — and so the memo's
/// check-then-insert is atomic by construction rather than by an unstated
/// obligation on a second, separately locked global.
///
/// Scope is this process only: it does not serialize against a separately
/// spawned config process, a running `vorpal start agent`, or the Go/
/// TypeScript SDKs writing the same `credentials.json`, and the memo is
/// neither durable nor visible to them. `write_credentials_secure`'s atomic
/// write independently keeps every one of those readers from ever observing
/// a torn file; only the refresh-exchange race is process-local.
static CREDENTIALS_REFRESH: Mutex<RefreshState> = Mutex::const_new(RefreshState::new());

/// Decides whether the stored access token must be refreshed before use.
///
/// Pure in its three inputs, so the policy is checkable without a
/// credentials file, the process lock, or a network round trip.
///
/// A future-dated `issued_at` (clock skew, or a hostile file — the value is
/// file-sourced and unvalidated, VPL-183 AB-5) means the token's real age is
/// unknown, and unknown age fails toward refreshing. Clamping it to zero
/// makes a stale token look freshly issued and suppresses a refresh that is
/// genuinely due, which is the opposite of fail-safe.
///
/// The head room is `min(300, expires_in / 2)`: five minutes for a normally
/// long-lived token, proportional below that. A flat 300 is discontinuous at
/// the threshold — it makes this check true one second after a rotation for
/// any `expires_in` a little above it, and unconditionally true at or below
/// it, so every caller decides independently to rotate and the rotations
/// amplify each other (VPL-183 AB-4).
///
/// Rust, Go (`sdk/go/pkg/config/context.go`'s `needsRefresh`) and TypeScript
/// (`sdk/typescript/src/context.ts`'s `needsRefresh`) deliberately agree on
/// this skew-safe direction (VPL-189). A later "restore cross-SDK parity"
/// pass must not revert any of the three back to suppress-on-skew — the
/// divergence from the pre-VPL-183 behavior is intentional, not drift.
fn needs_refresh(issued_at: u64, expires_in: u64, now: u64) -> bool {
    if issued_at > now {
        return true;
    }

    let token_age = now - issued_at;
    let refresh_window = (expires_in / 2).min(300);

    token_age + refresh_window >= expires_in
}

/// Applies a completed exchange to `credentials` and writes the result to
/// `path`. Returns `Err` without committing anything when the IdP's response
/// is unusable.
async fn commit_refreshed_credentials(
    credentials: &mut VorpalCredentials,
    issuer: &str,
    path: &Path,
    refreshed: RefreshedToken,
) -> Result<()> {
    let (access_token, expires_in, issued_at, rotated_refresh_token) = refreshed;

    // A zero lifetime can never satisfy the refresh window, so storing it
    // would make every later call rotate again — and each rotation mints a
    // fresh token value, which the spent-token memo cannot bound (VPL-183
    // AB-8). Refuse the record and make the user re-login instead.
    if expires_in == 0 {
        bail!(
            "OAuth refresh for issuer {} returned a token with a zero lifetime. Please run: vorpal login --issuer {}",
            issuer,
            issuer
        );
    }

    let issuer_creds = credentials
        .issuer
        .get_mut(issuer)
        .ok_or_else(|| anyhow!("no credentials for issuer: {}", issuer))?;

    apply_token_refresh(
        issuer_creds,
        access_token,
        expires_in,
        issued_at,
        rotated_refresh_token,
    );

    // Save updated credentials with mode 0o600 enforced on the temp file.
    let credentials_json = serde_json::to_string_pretty(credentials)?;

    write_credentials_secure(path, credentials_json.as_bytes()).await
}

/// The error every arm that spends the stored refresh token returns: what
/// went wrong locally, that the grant is over, and the one command that
/// restores it.
///
/// `cause` is inlined into the message rather than left to the source chain
/// alone. Every production caller of [`client_auth_header`] re-wraps this
/// error with `map_err` and a `Display` format (`cli/src/command/build.rs`,
/// `run.rs`, `inspect.rs`, `start/agent.rs`), and anyhow's `Display` prints
/// only the outermost context — so a cause left underneath is invisible
/// everywhere a user reads it. The chain is still attached for `{:#}` and
/// `Debug` renderings.
fn spent_grant_error(issuer: &str, summary: &str, cause: anyhow::Error) -> anyhow::Error {
    let message = format!(
        "{} ({:#}). Please run: vorpal login --issuer {}",
        summary, cause, issuer
    );

    cause.context(message)
}

/// Core of [`client_auth_header`], taking the credentials path, the refresh
/// operation and a clock as parameters so it is testable without touching
/// the real `/var/lib/vorpal/key/credentials.json`, performing network I/O,
/// or depending on the wall clock. Deliberately private and not configurable
/// from any production entry point (env var, global override) — see
/// `client_auth_header`.
///
/// `clock` is a closure, not a plain value, and it is called only after
/// [`CREDENTIALS_REFRESH`] is held — never before. A value sampled before
/// requesting the lock can predate a still-in-flight winner's later commit;
/// a waiter that then compares its stale, pre-lock reading against the
/// winner's freshly committed `issued_at` sees a future-dated token and
/// refreshes again, once per waiter (VPL-283 AB-283-1 / C-1). Every candidate
/// fix that instead clamps or ignores a future-dated `issued_at` in
/// [`needs_refresh`] is fail-open (VPL-283 C-2) — this seam fixes the sample
/// point, not the policy.
///
/// The precondition that makes the post-lock sample sufficient: the wall
/// clock does not run backwards during an exchange. `SystemTime` is not
/// monotonic, so a backwards NTP correction or an operator stepping the
/// clock while an exchange is in flight puts `issued_at` in the next waiter's
/// future again and costs one surplus exchange. It is bounded to one, because
/// that waiter's own commit re-mints `issued_at` from the clock it just read
/// (VPL-283 SEC-283-2, accepted residual risk).
async fn client_auth_header_at(
    credentials_path: &Path,
    registry: &str,
    refresher: &dyn TokenRefresher,
    clock: &(dyn Fn() -> Result<u64> + Send + Sync),
) -> Result<Option<MetadataValue<Ascii>>> {
    // Acquired before the existence check and the read below, and held
    // through the write: see CREDENTIALS_REFRESH's doc comment.
    let mut state = CREDENTIALS_REFRESH.lock().await;

    // Read here, strictly after the lock — see this function's doc comment.
    let now = clock()?;

    if !credentials_path.exists() {
        return Ok(None);
    }

    let credentials_data = read(credentials_path).await?;
    let mut credentials: VorpalCredentials = serde_json::from_slice(&credentials_data)?;

    // Cloned rather than borrowed: `commit_refreshed_credentials` below takes
    // `&mut credentials` for the whole struct (not just `credentials.issuer`),
    // so a borrow from `credentials.registry` would still be live across it.
    let Some(registry_issuer) = credentials.registry.get(registry).cloned() else {
        return Ok(None);
    };

    let issuer_creds = credentials
        .issuer
        .get(&registry_issuer)
        .ok_or_else(|| anyhow!("no credentials for issuer: {registry_issuer}"))?;

    let needs_refresh = needs_refresh(issuer_creds.issued_at, issuer_creds.expires_in, now);

    if needs_refresh {
        // Skip refresh if no refresh token available (user must re-login)
        if issuer_creds.refresh_token.is_empty() {
            return Err(anyhow!(
                "Access token expired and no refresh token available. Please run: vorpal login --issuer {registry_issuer}"
            ));
        }

        // A prior caller may already have put this exact stored token value
        // on the wire. The file it left behind is byte-identical either way,
        // so the memo is the only thing that can tell the two apart.
        let refresh_token_digest = digest(issuer_creds.refresh_token.as_str());

        if state.spent.contains(&refresh_token_digest) {
            return Err(anyhow!(
                "OAuth refresh-token exchange already failed for the stored token. Please run: vorpal login --issuer {}",
                registry_issuer
            ));
        }

        let exchange = refresher
            .refresh(
                issuer_creds.audience.as_deref(),
                &issuer_creds.client_id,
                &registry_issuer,
                &issuer_creds.refresh_token,
            )
            .await;

        let refreshed = match exchange {
            Ok(refreshed) => refreshed,

            // The token never left the process: the fault was local, the
            // stored token is untouched, and a later caller may use it.
            Err(RefreshFailure::NotSent(err)) => return Err(err),

            // The token reached the IdP, which may have consumed it whatever
            // came back. Spend it rather than let the next caller replay it.
            Err(RefreshFailure::Sent(err)) => {
                state.spent.insert(refresh_token_digest);

                return Err(spent_grant_error(
                    &registry_issuer,
                    &format!(
                        "The OAuth refresh-token exchange for issuer {} failed after the token had been sent, so the stored refresh token is no longer usable",
                        registry_issuer
                    ),
                    err,
                ));
            }
        };

        // No await is introduced between the exchange above and the write
        // below: the sequence stays synchronous so the window in which a
        // killed task loses the rotated token to disk (accepted residual
        // risk) does not widen.
        if let Err(err) = commit_refreshed_credentials(
            &mut credentials,
            &registry_issuer,
            credentials_path,
            refreshed,
        )
        .await
        {
            // The exchange happened and nothing was committed, so the file
            // still names a token the IdP has already rotated away. This is
            // the same replay hazard as an outright failure and it is spent
            // for the same reason.
            state.spent.insert(refresh_token_digest);

            // Spending the token ends the grant, so say so here: without it
            // the reader sees only the local cause (a temp-file path, a full
            // disk) and concludes the problem is disk state, while the actual
            // state is a credential that no retry can revive.
            return Err(spent_grant_error(
                &registry_issuer,
                &format!(
                    "Refreshed credentials for issuer {} could not be saved, so the stored refresh token is no longer usable",
                    registry_issuer
                ),
                err,
            ));
        }
    }

    // Get the access token
    let access_token = &credentials
        .issuer
        .get(&registry_issuer)
        .ok_or_else(|| anyhow!("no credentials for issuer: {registry_issuer}"))?
        .access_token;

    let header = format!("Bearer {access_token}")
        .parse()
        .map_err(|e| anyhow!("failed to parse Bearer token: {e}"))?;

    Ok(Some(header))
}

/// The only wall-clock reading any refresh decision is made against: seconds
/// since the Unix epoch. Both the refresh decision and the `issued_at` it
/// mints come through here, so the answer to "what clock does this module
/// decide on?" is this one function.
///
/// Not the only `SystemTime::now` in the file: `#[cfg(test)]` code reads the
/// clock directly twice on purpose — for a scratch directory's unique name,
/// and for a ground truth the live-wiring test must not take from the
/// function it exercises. Neither decides refresh behaviour.
fn system_now() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

/// Builds the `authorization: Bearer <token>` gRPC metadata header for
/// `registry`, refreshing the stored access token first if it is within five
/// minutes of expiry. Returns `None` when no credentials file exists or the
/// registry has no stored issuer, in which case the request proceeds
/// unauthenticated.
///
/// # Errors
///
/// Returns an error if the credentials file cannot be read or parsed; if the
/// registry's issuer has no stored credentials; if the access token is
/// expired and no refresh token is available; if the token refresh request
/// fails; if the refreshed credentials cannot be saved; or if the resulting
/// header value fails to parse.
pub async fn client_auth_header(registry: &str) -> Result<Option<MetadataValue<Ascii>>> {
    client_auth_header_live(&get_key_credentials_path(), registry).await
}

/// The production wiring of [`client_auth_header`] — the real IdP exchange
/// and the real clock — with only the credentials path left as a parameter,
/// so a test can drive that wiring instead of trusting it by inspection.
///
/// `system_now` is passed as the clock itself, never called here: sampling it
/// at this level and passing the value down is exactly the pre-lock sample
/// that caused the refresh storm (VPL-283 C-1), and this is the function
/// where that regression would reappear.
async fn client_auth_header_live(
    credentials_path: &Path,
    registry: &str,
) -> Result<Option<MetadataValue<Ascii>>> {
    client_auth_header_at(credentials_path, registry, &LiveTokenRefresher, &system_now).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{atomic::AtomicU32, Arc};

    fn unix_now() -> u64 {
        system_now().expect("system clock is after the Unix epoch")
    }

    /// Epoch seconds read without going through `system_now`, for the one
    /// test that drives production's real clock end to end: a fixture stamped
    /// by the very function under test moves with it, so a clock that never
    /// advances compares equal to itself and the test stays green.
    fn independent_unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_secs()
    }

    /// A previously-nonexistent scratch directory holding one test's
    /// `credentials.json`, removed when the test's binding goes out of scope.
    ///
    /// `label` is the test's own name. Deriving the directory, the issuer and
    /// the token values from it keeps each test's entries in the process-wide
    /// spent-token memo disjoint from every other test's, which is what makes
    /// the suite order-independent under Rust's parallel test threads. The
    /// memo has process lifetime by design and is deliberately not reset
    /// between tests: a reset would race the tests running beside it.
    struct ScratchCredentials {
        dir: PathBuf,
        label: String,
        path: PathBuf,
    }

    impl ScratchCredentials {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "vorpal-creds-{}-{}-{}",
                label,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));

            std::fs::create_dir_all(&dir).expect("create scratch dir");

            let path = dir.join("credentials.json");

            Self {
                dir,
                label: label.to_string(),
                path,
            }
        }

        /// This test's issuer name, unique to it.
        fn issuer(&self) -> String {
            format!("issuer-{}", self.label)
        }

        /// A token value unique to this test and `name`.
        fn token(&self, name: &str) -> String {
            format!("{}-{}", name, self.label)
        }

        fn write_fixture(&self, expires_in: u64, issued_at: u64, refresh_token: &str) {
            let mut issuer = BTreeMap::new();

            issuer.insert(
                self.issuer(),
                VorpalCredentialsContent {
                    access_token: "old-access".to_string(),
                    audience: None,
                    client_id: "client-1".to_string(),
                    expires_in,
                    issued_at,
                    refresh_token: refresh_token.to_string(),
                    scopes: vec!["openid".to_string()],
                },
            );

            let mut registry = BTreeMap::new();

            registry.insert("registry-1".to_string(), self.issuer());

            let credentials = VorpalCredentials { issuer, registry };

            std::fs::write(&self.path, serde_json::to_vec(&credentials).unwrap())
                .expect("write fixture");
        }

        fn stored(&self) -> VorpalCredentialsContent {
            let bytes = std::fs::read(&self.path).expect("read credentials");

            let mut credentials: VorpalCredentials =
                serde_json::from_slice(&bytes).expect("parse credentials");

            credentials
                .issuer
                .remove(&self.issuer())
                .expect("issuer present")
        }

        /// Temp-file names left behind in the scratch directory.
        fn leftover_temp_files(&self) -> Vec<String> {
            std::fs::read_dir(&self.dir)
                .expect("read scratch dir")
                .map(|entry| entry.expect("dir entry").file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect()
        }
    }

    impl Drop for ScratchCredentials {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700));
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    type Exchange = std::result::Result<RefreshedToken, RefreshFailure>;

    /// Counts exchanges and answers each with a scripted outcome, so a test
    /// can pin how many times the real entry point reached the IdP.
    struct ScriptedRefresher {
        calls: AtomicU32,
        respond: Box<dyn Fn(u32) -> Exchange + Send + Sync>,
    }

    impl ScriptedRefresher {
        fn new(respond: impl Fn(u32) -> Exchange + Send + Sync + 'static) -> Self {
            Self {
                calls: AtomicU32::new(0),
                respond: Box::new(respond),
            }
        }

        fn calls(&self) -> u32 {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[tonic::async_trait]
    impl TokenRefresher for ScriptedRefresher {
        async fn refresh(
            &self,
            _audience: Option<&str>,
            _client_id: &str,
            _issuer: &str,
            _refresh_token: &str,
        ) -> Exchange {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);

            (self.respond)(call)
        }
    }

    fn sample_creds() -> VorpalCredentialsContent {
        VorpalCredentialsContent {
            access_token: "old-access".to_string(),
            audience: Some("aud-1".to_string()),
            client_id: "client-1".to_string(),
            expires_in: 3600,
            issued_at: 1_700_000_000,
            refresh_token: "old-refresh".to_string(),
            scopes: vec!["openid".to_string(), "offline_access".to_string()],
        }
    }

    #[test]
    fn apply_token_refresh_replaces_refresh_when_rotated() {
        let mut creds = sample_creds();

        apply_token_refresh(
            &mut creds,
            "new-access".to_string(),
            7200,
            1_700_000_500,
            Some("rotated-refresh".to_string()),
        );

        assert_eq!(creds.access_token, "new-access");
        assert_eq!(creds.expires_in, 7200);
        assert_eq!(creds.issued_at, 1_700_000_500);
        assert_eq!(creds.refresh_token, "rotated-refresh");
        assert_eq!(creds.audience.as_deref(), Some("aud-1"));
        assert_eq!(creds.client_id, "client-1");
        assert_eq!(creds.scopes, vec!["openid", "offline_access"]);
    }

    #[test]
    fn apply_token_refresh_keeps_refresh_when_not_rotated() {
        let mut creds = sample_creds();

        apply_token_refresh(
            &mut creds,
            "new-access".to_string(),
            7200,
            1_700_000_500,
            None,
        );

        assert_eq!(creds.access_token, "new-access");
        assert_eq!(creds.expires_in, 7200);
        assert_eq!(creds.issued_at, 1_700_000_500);
        assert_eq!(creds.refresh_token, "old-refresh");
        assert_eq!(creds.audience.as_deref(), Some("aud-1"));
        assert_eq!(creds.client_id, "client-1");
        assert_eq!(creds.scopes, vec!["openid", "offline_access"]);
    }

    #[test]
    fn normalize_rotated_refresh_token_some_nonempty_passes_through() {
        assert_eq!(
            normalize_rotated_refresh_token(Some("rotated-refresh".to_string())),
            Some("rotated-refresh".to_string())
        );
    }

    #[test]
    fn normalize_rotated_refresh_token_some_empty_becomes_none() {
        assert_eq!(
            normalize_rotated_refresh_token(Some(String::new())),
            None,
            "empty-string refresh_token must be treated as not-rotated for parity with Go/TS"
        );
    }

    #[test]
    fn normalize_rotated_refresh_token_none_passes_through() {
        assert_eq!(normalize_rotated_refresh_token(None), None);
    }

    #[test]
    fn write_credentials_secure_creates_file_with_mode_0o600() -> Result<()> {
        let scratch = ScratchCredentials::new("mode");

        // Sanity: the path must not pre-exist — we are testing file birth, not
        // an inherited mode from a pre-created 0o600 file.
        assert!(
            !scratch.path.exists(),
            "test path must be previously-nonexistent"
        );

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(write_credentials_secure(
            &scratch.path,
            b"{\"hello\":\"world\"}",
        ))?;

        let mode = std::fs::metadata(&scratch.path)?.permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credentials file must be born 0o600, got {:o}",
            mode & 0o777
        );

        Ok(())
    }

    #[test]
    fn apply_token_refresh_persists_through_serde_roundtrip() -> Result<()> {
        let mut creds = sample_creds();

        apply_token_refresh(
            &mut creds,
            "new-access".to_string(),
            7200,
            1_700_000_500,
            Some("rotated-refresh".to_string()),
        );

        let json = serde_json::to_string(&creds)?;
        let parsed: VorpalCredentialsContent = serde_json::from_str(&json)?;

        assert_eq!(parsed.access_token, "new-access");
        assert_eq!(parsed.refresh_token, "rotated-refresh");
        assert_eq!(parsed.expires_in, 7200);
        assert_eq!(parsed.issued_at, 1_700_000_500);
        assert_eq!(parsed.audience.as_deref(), Some("aud-1"));
        assert_eq!(parsed.client_id, "client-1");
        assert_eq!(parsed.scopes, vec!["openid", "offline_access"]);

        Ok(())
    }

    #[tokio::test]
    async fn write_credentials_secure_overwrites_existing_file_mode_to_0o600() {
        let scratch = ScratchCredentials::new("mode-regression");

        // Pre-create the destination at 0o644, the mode a naive
        // `File::create`-based temp writer would carry onto the destination
        // via `rename` (A-4). The fix must not inherit it.
        std::fs::write(&scratch.path, b"{\"stale\":true}").expect("write pre-existing file");
        std::fs::set_permissions(&scratch.path, std::fs::Permissions::from_mode(0o644))
            .expect("set pre-existing mode to 0o644");

        write_credentials_secure(&scratch.path, b"{\"hello\":\"world\"}")
            .await
            .expect("write credentials");

        let mode = std::fs::metadata(&scratch.path)
            .expect("stat credentials")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credentials file must be 0o600 after overwriting a pre-existing 0o644 file, got {:o}",
            mode & 0o777
        );

        let contents = std::fs::read(&scratch.path).expect("read credentials");
        assert_eq!(contents, b"{\"hello\":\"world\"}");
    }

    #[tokio::test]
    async fn client_auth_header_at_serializes_concurrent_refresh() {
        // N concurrent callers deciding "refresh needed" against the same
        // credentials file must trigger exactly one refresh exchange, and the
        // file left behind must hold the single winner's rotated token — not
        // a stale snapshot written by a waiter who decided "refresh needed"
        // before the guard (A-2).
        let scratch = ScratchCredentials::new("concurrency");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        // Token 4 minutes from expiry: token_age + 300 >= expires_in.
        let expires_in = 3600u64;

        scratch.write_fixture(
            expires_in,
            now - (expires_in - 240),
            &scratch.token("old-refresh"),
        );

        let refresher = Arc::new(ScriptedRefresher::new(move |_| {
            Ok((
                "rotated-access".to_string(),
                3600,
                now,
                Some(rotated.clone()),
            ))
        }));

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let refresher = refresher.clone();

            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref(), &|| Ok(now)).await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("client_auth_header_at failed");
        }

        assert_eq!(
            refresher.calls(),
            1,
            "exactly one refresh exchange must occur for N concurrent callers"
        );

        let stored = scratch.stored();

        assert_eq!(
            stored.refresh_token,
            scratch.token("rotated-refresh"),
            "stored refresh token must be the single winner's rotated token, not a stale replay"
        );
        assert_eq!(stored.access_token, "rotated-access");
    }

    #[tokio::test]
    async fn client_auth_header_at_never_replays_a_token_whose_refresh_failed_to_persist() {
        // The exchange succeeds and the IdP rotates the token, but the write
        // never lands, so `credentials.json` still names a token the IdP has
        // already consumed. Every waiter that re-reads that file must be told
        // the token is spent rather than replay it (VPL-183 C1): the outcome
        // of the exchange is what must be remembered, not the failure of it.
        let scratch = ScratchCredentials::new("persist-failure");
        let now = unix_now();
        let original = scratch.token("old-refresh");
        let rotated = scratch.token("rotated-refresh");

        scratch.write_fixture(3600, now - 3360, &original);

        // Make the directory unwritable so `create_new` on the temp file
        // fails after the exchange has already happened.
        std::fs::set_permissions(&scratch.dir, std::fs::Permissions::from_mode(0o500))
            .expect("make scratch dir unwritable");

        let refresher = Arc::new(ScriptedRefresher::new(move |_| {
            Ok((
                "rotated-access".to_string(),
                3600,
                now,
                Some(rotated.clone()),
            ))
        }));

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let refresher = refresher.clone();

            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref(), &|| Ok(now)).await
            });
        }

        let mut errors = 0;
        let mut commit_failures = 0;

        while let Some(result) = tasks.join_next().await {
            let error = result
                .expect("task panicked")
                .expect_err("a refresh that never reached disk must not be reported as success");

            // Display, not `{:#}` — every production caller re-wraps this
            // error with a `Display` format, so the outermost rendering is
            // the only one a user ever reads. The one caller that hit the
            // commit-failure arm must see both what failed locally and that
            // the grant is over; the other seven are told the token is spent.
            let rendered = error.to_string();

            if rendered.contains("could not be saved") {
                commit_failures += 1;

                assert!(
                    rendered.contains("Permission denied"),
                    "the local cause must survive into the message a caller prints, got: {}",
                    rendered
                );
                assert!(
                    rendered.contains("no longer usable")
                        && rendered.contains("vorpal login --issuer"),
                    "a spent grant must name the remedy, got: {}",
                    rendered
                );
            }

            errors += 1;
        }

        assert_eq!(errors, 8);
        assert_eq!(
            commit_failures, 1,
            "exactly one caller reaches the commit-failure arm; the rest are answered by the memo"
        );

        assert_eq!(
            refresher.calls(),
            1,
            "a consumed refresh token must not be exchanged again after the write failed"
        );

        std::fs::set_permissions(&scratch.dir, std::fs::Permissions::from_mode(0o700))
            .expect("restore scratch dir permissions");

        assert_eq!(
            scratch.stored().refresh_token,
            original,
            "the stored token is unchanged, which is exactly why the memo has to remember it"
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_retries_a_refresh_that_never_reached_the_idp() {
        // A discovery or DNS failure aborts the exchange before the token is
        // on the wire, so the stored token is untouched and a later call must
        // still be able to use it. Latching on those failures instead bricks
        // every authenticated call in a long-lived process until it restarts
        // (VPL-183 C2).
        let scratch = ScratchCredentials::new("not-sent");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        scratch.write_fixture(3600, now - 3360, &scratch.token("old-refresh"));

        let refresher = ScriptedRefresher::new(move |call| {
            if call == 0 {
                return Err(RefreshFailure::NotSent(anyhow!(
                    "error sending request for url (dns error)"
                )));
            }

            Ok((
                "rotated-access".to_string(),
                3600,
                now,
                Some(rotated.clone()),
            ))
        });

        client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
            .await
            .expect_err("the first call fails locally");

        let header = client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
            .await
            .expect("a token that never left the process must still be usable");

        assert!(header.is_some());
        assert_eq!(
            refresher.calls(),
            2,
            "the second call must attempt the exchange the first one never made"
        );
        assert_eq!(
            scratch.stored().refresh_token,
            scratch.token("rotated-refresh")
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_refreshes_a_new_token_after_an_older_one_is_spent() {
        // The memo is keyed on the refresh-token value, not the issuer: a
        // token burned by a failed exchange must not poison a different token
        // for the same issuer, which is what happens after a re-login
        // (VPL-183 C12).
        let scratch = ScratchCredentials::new("value-keyed-memo");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        scratch.write_fixture(3600, now - 3360, &scratch.token("burned-refresh"));

        let refresher = ScriptedRefresher::new(move |call| {
            if call == 0 {
                return Err(RefreshFailure::Sent(anyhow!("simulated IdP rejection")));
            }

            Ok((
                "rotated-access".to_string(),
                3600,
                now,
                Some(rotated.clone()),
            ))
        });

        client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
            .await
            .expect_err("the first exchange fails and spends its token");

        // A re-login replaces the stored token with a different value.
        scratch.write_fixture(3600, now - 3360, &scratch.token("fresh-refresh"));

        let header = client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
            .await
            .expect("a different stored token must still be exchangeable");

        assert!(header.is_some());
        assert_eq!(
            refresher.calls(),
            2,
            "spending one token value must not spend every token for that issuer"
        );
        assert_eq!(
            scratch.stored().refresh_token,
            scratch.token("rotated-refresh")
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_stops_rotating_a_zero_lifetime_token() {
        // An IdP reporting `expires_in = 0` hands back a record that can never
        // satisfy the refresh window, and each rotation mints a fresh token
        // value the memo cannot bound, so storing it produces an unbounded
        // rotation storm (VPL-183 C4). Refuse the record instead.
        let scratch = ScratchCredentials::new("zero-lifetime");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        scratch.write_fixture(0, now, &scratch.token("old-refresh"));

        let refresher = ScriptedRefresher::new(move |call| {
            Ok((
                "rotated-access".to_string(),
                0,
                now,
                Some(format!("{}-{}", rotated, call)),
            ))
        });

        for _ in 0..5 {
            client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
                .await
                .expect_err("a zero-lifetime token is unusable and must say so");
        }

        assert_eq!(
            refresher.calls(),
            1,
            "a zero-lifetime response must not be stored and rotated again by every later caller"
        );
        assert_eq!(
            scratch.stored().refresh_token,
            scratch.token("old-refresh"),
            "an unusable response must not be committed"
        );
    }

    #[test]
    fn needs_refresh_treats_a_future_issued_at_as_unknown_age() {
        // Clock skew (or a hostile file) must fail toward refreshing, never
        // away from it: clamping the age to zero makes a stale token look
        // freshly issued and suppresses a refresh that is due.
        assert!(needs_refresh(1_700_003_600, 3600, 1_700_000_000));
    }

    #[test]
    fn needs_refresh_holds_off_on_a_freshly_issued_token_at_every_lifetime() {
        // The window is continuous across the 300s threshold: a token issued
        // `now` never immediately re-qualifies, whatever its lifetime. A flat
        // 300s window fails this at 301 and below.
        let now = 1_700_000_000;

        for expires_in in [1, 2, 60, 299, 300, 301, 600, 3600] {
            assert!(
                !needs_refresh(now, expires_in, now),
                "a token issued now with expires_in {} must not be due for refresh",
                expires_in
            );
        }
    }

    #[test]
    fn needs_refresh_is_due_once_the_window_is_reached() {
        let now = 1_700_000_000;

        // (expires_in, age at which the refresh becomes due).
        for (expires_in, due_at) in [
            (60u64, 30u64),
            (300, 150),
            (301, 151),
            (600, 300),
            (3600, 3300),
        ] {
            assert!(
                !needs_refresh(now - (due_at - 1), expires_in, now),
                "expires_in {} must not be due one second early",
                expires_in
            );
            assert!(
                needs_refresh(now - due_at, expires_in, now),
                "expires_in {} must be due at age {}",
                expires_in,
                due_at
            );
        }
    }

    #[test]
    fn needs_refresh_is_due_for_a_zero_lifetime_token() {
        // Already expired on arrival: the record is unusable, and the caller
        // finds that out by attempting the refresh.
        assert!(needs_refresh(1_700_000_000, 0, 1_700_000_000));
    }

    #[tokio::test]
    async fn client_auth_header_at_refreshes_despite_future_issued_at() {
        // AC4 (VPL-183, reversing a wrong-direction test pinned by VPL-129):
        // `issued_at` is file-sourced and unvalidated (AB-5). A future-dated
        // value must not underflow `now - issued_at` (panics in the
        // test/debug profile; wraps in release) — that part is unchanged —
        // but a future-dated `issued_at` also means the token's real age is
        // unknown, and unknown age must fail toward refreshing rather than
        // being clamped to "just issued". The previous version of this test
        // asserted the opposite with a refresher that panicked if called at
        // all, which pinned the fail-open direction into the suite.
        let scratch = ScratchCredentials::new("future-issued-at");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        scratch.write_fixture(3600, now + 3600, &scratch.token("old-refresh"));

        let refresher = ScriptedRefresher::new(move |_| {
            Ok((
                "rotated-access".to_string(),
                3600,
                now,
                Some(rotated.clone()),
            ))
        });

        let header = client_auth_header_at(&scratch.path, "registry-1", &refresher, &|| Ok(now))
            .await
            .expect("must not panic or error on a future-dated issued_at");

        assert!(
            header.is_some(),
            "a refresh attempted for a skewed token must still yield an auth header"
        );
        assert_eq!(
            refresher.calls(),
            1,
            "a future-dated issued_at must not suppress a refresh"
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_never_replays_refresh_token_after_a_failed_exchange() {
        // AC1 (VPL-183 C-1): a failed or timed-out exchange must be
        // memoized as a terminal outcome for that stored refresh-token
        // value, not merely serialized in time. Without the memo, every
        // waiter behind the lock re-reads the byte-identical post-failure
        // file, re-decides "needs refresh", and replays the same one-time
        // refresh token against the IdP (AB-1) — this asserts exactly one
        // exchange attempt occurs for 8 concurrent callers racing a
        // guaranteed failure, and that the stored token is never mutated.
        let scratch = ScratchCredentials::new("failed-exchange");
        let now = unix_now();
        let original = scratch.token("old-refresh");

        // Token 4 minutes from expiry: token_age + 300 >= expires_in.
        let expires_in = 3600u64;

        scratch.write_fixture(expires_in, now - (expires_in - 240), &original);

        let refresher = Arc::new(ScriptedRefresher::new(|_| {
            Err(RefreshFailure::Sent(anyhow!("simulated IdP timeout")))
        }));

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let refresher = refresher.clone();

            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref(), &|| Ok(now)).await
            });
        }

        let mut error_count = 0;
        let mut sent_failures = 0;

        while let Some(result) = tasks.join_next().await {
            let error = result
                .expect("task panicked")
                .expect_err("every caller must observe the failure, not a stale success");

            let rendered = error.to_string();

            // The caller that reached the IdP ends the grant here, so its
            // message says so — a bare transport complaint sends the reader
            // looking for a network fault while the state is a dead grant.
            if rendered.contains("simulated IdP timeout") {
                sent_failures += 1;

                assert!(
                    rendered.contains("no longer usable")
                        && rendered.contains("vorpal login --issuer"),
                    "a spent grant must name the remedy, got: {}",
                    rendered
                );
            }

            error_count += 1;
        }

        assert_eq!(error_count, 8);
        assert_eq!(
            sent_failures, 1,
            "exactly one caller reaches the IdP; the rest are answered by the memo"
        );

        assert_eq!(
            refresher.calls(),
            1,
            "exactly one exchange attempt must occur even though 8 callers observed a failure"
        );

        assert_eq!(
            scratch.stored().refresh_token,
            original,
            "the stored refresh token must be untouched after a failed exchange"
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_does_not_scale_short_expiry_rotations_with_callers() {
        // AC2 (VPL-183 C-2): a fixed 300s refresh window makes
        // `token_age + 300 >= expires_in` unconditionally true whenever
        // `expires_in <= 300`, so every waiter independently decides
        // "needs refresh" (AB-4). Assert the exchange count does not scale
        // with the number of concurrent callers, and that a second
        // sequential call right after a successful refresh does not
        // immediately rotate again.
        let scratch = ScratchCredentials::new("short-expiry");
        let now = unix_now();
        let rotated = scratch.token("rotated-refresh");

        // Well outside any window, regardless of the window's size.
        scratch.write_fixture(60, now - 3600, &scratch.token("old-refresh"));

        let refresher = Arc::new(ScriptedRefresher::new(move |_| {
            Ok(("rotated-access".to_string(), 60, now, Some(rotated.clone())))
        }));

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let refresher = refresher.clone();

            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref(), &|| Ok(now)).await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("client_auth_header_at failed");
        }

        assert_eq!(
            refresher.calls(),
            1,
            "exchange count must not scale with the number of concurrent callers for a short-lived token"
        );

        // A second sequential call right after the successful refresh must
        // not immediately rotate again: the freshly issued token
        // (issued_at ~ now, expires_in 60) must fall outside the
        // proportional refresh window.
        client_auth_header_at(&scratch.path, "registry-1", refresher.as_ref(), &|| Ok(now))
            .await
            .expect("second call must succeed");

        assert_eq!(
            refresher.calls(),
            1,
            "a freshly rotated short-lived token must not be rotated again immediately"
        );
    }

    #[tokio::test]
    async fn client_auth_header_at_samples_the_clock_after_the_refresh_lock_not_before() {
        // AC8 (VPL-283 C-1), reproducing judge-correctness's PROBE674-A: a
        // `now` sampled before a waiter even requested CREDENTIALS_REFRESH
        // can predate a still-in-flight winner's later commit. This clock
        // models real elapsed time — it starts at the value every waiter
        // would have sampled before queuing on the lock, and the winning
        // exchange advances it to a strictly later value, exactly as a real
        // wall clock does while the other 7 wait. A waiter whose `now` read
        // happens only after it holds the lock necessarily observes the
        // winner's advance (the winner cannot release the lock until its
        // commit, which bumps the clock, has already happened); a waiter
        // that read the clock before requesting the lock cannot.
        let scratch = ScratchCredentials::new("clock-skew-storm");
        let before_lock = unix_now();
        let rotated = scratch.token("rotated-refresh");

        // Token 4 minutes from expiry: token_age + 300 >= expires_in, so a
        // waiter evaluating against a correctly current clock still finds a
        // refresh due before the winner commits.
        let expires_in = 3600u64;

        scratch.write_fixture(
            expires_in,
            before_lock - (expires_in - 240),
            &scratch.token("old-refresh"),
        );

        let sim_clock = Arc::new(AtomicU64::new(before_lock));
        let refresher_clock = sim_clock.clone();

        let refresher = Arc::new(ScriptedRefresher::new(move |_| {
            // Every exchange mints its issued_at from a clock reading one
            // second later than the last, the same self-healing shape
            // `refresh_access_token` uses against the real wall clock
            // (`SystemTime::now()`), and advances the shared clock to match
            // — modeling time passing while an exchange is in flight.
            let issued_at = refresher_clock.fetch_add(1, Ordering::SeqCst) + 1;

            Ok((
                "rotated-access".to_string(),
                3600,
                issued_at,
                Some(rotated.clone()),
            ))
        }));

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let refresher = refresher.clone();
            let clock = sim_clock.clone();

            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref(), &move || {
                    Ok(clock.load(Ordering::SeqCst))
                })
                .await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("client_auth_header_at failed");
        }

        assert_eq!(
            refresher.calls(),
            1,
            "exactly one exchange must occur for 8 waiters racing the refresh lock, got {}",
            refresher.calls()
        );
    }

    #[tokio::test]
    async fn client_auth_header_live_reads_the_clock_after_the_lock_not_before() {
        // The seam test above pins `client_auth_header_at`; this one pins the
        // production wiring that feeds it, which is where this round's defect
        // actually lived — a caller that samples `system_now()` once and
        // passes the value down reintroduces it with the seam untouched.
        //
        // Real wall clock, no simulated one: hold the refresh lock, let the
        // call queue on it, and stamp the credentials with a reading taken a
        // second later than the caller's own start. A caller that reads the
        // clock only after the lock sees a token issued in its past and needs
        // no refresh at all; a caller that read it before sees a future-dated
        // token, calls the live IdP, and fails.
        let scratch = ScratchCredentials::new("live-clock-wiring");
        let path = scratch.path.clone();

        let state = CREDENTIALS_REFRESH.lock().await;

        let call = tokio::spawn(async move { client_auth_header_live(&path, "registry-1").await });

        // Long enough that the spawned call has been polled (so a pre-lock
        // sample would already have happened) and that the stamp below is a
        // strictly later whole second.
        //
        // Real elapsed time is the synchronization here, which no other test
        // in this module needs: tokio's pause/advance cannot drive a test
        // whose whole subject is the real `system_now`, and the margin is a
        // guess at scheduler latency. The trade is deliberate and it does not
        // scale — a second test wanting this shape should take the ordering
        // from this one rather than add another second of wall time, and a
        // margin trimmed too far fails toward the mutant surviving, not
        // toward a spurious red.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        scratch.write_fixture(
            3600,
            independent_unix_now(),
            &scratch.token("stored-refresh"),
        );

        drop(state);

        let header = call
            .await
            .expect("task panicked")
            .expect("a token issued in the caller's past needs no refresh")
            .expect("credentials for registry-1 are present");

        assert_eq!(header, "Bearer old-access");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn write_credentials_secure_never_exposes_torn_content_to_a_concurrent_reader() {
        // AC3 (VPL-183 C-7 note: this targets write_credentials_secure
        // itself, which is what keeps AC3 satisfiable within this file's
        // declared scope). A reader racing the writer must always observe
        // either the previous full write or the next full write, never a
        // partial one. Mutant M6 (truncate-in-place, the pre-VPL-129 shape)
        // can leave a short or mixed-content read in that window; the
        // rename-based writer cannot, because the directory entry flips
        // atomically to a fully-written inode.
        const CONTENT_LEN: usize = 8192;
        const ITERATIONS: usize = 150;

        let scratch = ScratchCredentials::new("torn-read");
        let path = scratch.path.clone();
        let content_a = vec![b'A'; CONTENT_LEN];
        let content_b = vec![b'B'; CONTENT_LEN];

        std::fs::write(&path, &content_a).expect("seed file");

        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let writer_path = path.clone();
        let writer_content_a = content_a.clone();
        let writer_content_b = content_b.clone();
        let writer_done = done.clone();
        let writer = tokio::spawn(async move {
            for i in 0..ITERATIONS {
                let bytes = if i % 2 == 0 {
                    &writer_content_b
                } else {
                    &writer_content_a
                };
                write_credentials_secure(&writer_path, bytes)
                    .await
                    .expect("write credentials");
            }
            writer_done.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let reader_path = path.clone();
        let reader_done = done.clone();
        let reader = tokio::task::spawn_blocking(move || {
            let mut reads = 0usize;
            let mut torn = 0usize;
            loop {
                let finished = reader_done.load(std::sync::atomic::Ordering::SeqCst);
                if let Ok(bytes) = std::fs::read(&reader_path) {
                    reads += 1;
                    let is_pure_a = bytes == content_a;
                    let is_pure_b = bytes == content_b;
                    if !(is_pure_a || is_pure_b) {
                        torn += 1;
                    }
                }
                if finished {
                    break;
                }
            }
            (reads, torn)
        });

        writer.await.expect("writer task panicked");
        let (reads, torn) = reader.await.expect("reader task panicked");

        // A reader scheduled only after the writer finished would record one
        // read and no tears, and pass having tested nothing. Requiring more
        // reads than the writer made writes is what establishes the two
        // actually overlapped; the observed figure is around 70,000.
        assert!(
            reads > ITERATIONS,
            "reader made {reads} reads against {ITERATIONS} writes, so it never raced the writer"
        );
        assert_eq!(
            torn, 0,
            "reader observed {torn} torn/truncated reads out of {reads}"
        );
    }

    #[test]
    fn temp_file_guard_removes_the_file_when_dropped_still_armed() {
        // The mechanism on its own: an armed guard going out of scope is
        // exactly what happens on a cancelled task, a dropped `JoinSet`, or a
        // panic — none of those reach a post-await cleanup branch, but all of
        // them run destructors. The tests below pin its use.
        let scratch = ScratchCredentials::new("guard-armed");
        let temp_path = scratch.dir.join("credentials.json.12345.0.tmp");

        std::fs::write(&temp_path, b"live-refresh-token-bytes").expect("seed temp file");
        assert!(temp_path.exists());

        {
            let _guard = TempFileGuard::new(temp_path.clone());
            // guard drops here, still armed — simulating cancellation.
        }

        assert!(
            !temp_path.exists(),
            "an armed guard must remove the temp file when dropped"
        );
    }

    #[tokio::test]
    async fn write_credentials_secure_removes_its_temp_file_when_the_commit_fails() {
        // The guard has to be wired into the write path, not merely exist:
        // unwiring it leaves the suite green unless a test drives
        // `write_credentials_secure` itself and asserts the two properties
        // that matter — no temp file survives, and nothing on disk holds the
        // token bytes (VPL-183 C5).
        let scratch = ScratchCredentials::new("commit-failure");
        let token = scratch.token("live-refresh-token");
        let bytes = format!("{{\"refresh_token\":\"{}\"}}", token);

        // Make `rename` fail after the temp file has been written: a
        // non-empty directory cannot be replaced by a file.
        std::fs::create_dir(&scratch.path).expect("create directory at the destination path");
        std::fs::write(scratch.path.join("occupant"), b"x").expect("occupy the directory");

        write_credentials_secure(&scratch.path, bytes.as_bytes())
            .await
            .expect_err("renaming onto a non-empty directory must fail");

        assert!(
            scratch.leftover_temp_files().is_empty(),
            "a failed commit must leave no temp file behind, found {:?}",
            scratch.leftover_temp_files()
        );

        for entry in std::fs::read_dir(&scratch.dir).expect("read scratch dir") {
            let path = entry.expect("dir entry").path();

            if path.is_file() {
                let contents = std::fs::read(&path).expect("read leftover file");

                assert!(
                    !contents.windows(token.len()).any(|w| w == token.as_bytes()),
                    "{} still holds the refresh token after a failed commit",
                    path.display()
                );
            }
        }
    }

    #[tokio::test]
    async fn write_credentials_secure_leaves_no_temp_file_when_cancelled() {
        // Cancelling a write mid-flight is the case the guard exists for. The
        // file and its guard are created in one blocking unit, so there is no
        // instant at which the file exists unowned; arming around the open
        // instead lets the open land after the guard is already gone and the
        // temp file outlives the task (VPL-183 C8).
        let scratch = ScratchCredentials::new("cancelled-write");
        let bytes = vec![b'x'; 64 * 1024];

        for _ in 0..64 {
            let path = scratch.path.clone();
            let bytes = bytes.clone();

            let task = tokio::spawn(async move { write_credentials_secure(&path, &bytes).await });

            tokio::task::yield_now().await;
            task.abort();

            let _ = task.await;
        }

        // Aborting a task drops its future, but a `spawn_blocking` unit that
        // future was awaiting still runs to completion on a pool thread, and
        // the unlink happens when its result is dropped there. Cleanup is
        // therefore prompt but not synchronous with the abort, so wait for it
        // rather than sampling once.
        let mut leftovers = scratch.leftover_temp_files();

        for _ in 0..500 {
            if leftovers.is_empty() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(20)).await;

            leftovers = scratch.leftover_temp_files();
        }

        assert!(
            leftovers.is_empty(),
            "cancelled writes leaked temp files: {:?}",
            leftovers
        );
    }

    #[tokio::test]
    async fn write_credentials_secure_retries_past_an_eexist_collision() {
        // AC10 / C-4: `TEMP_FILE_COUNTER` resets to 0 on every process
        // start, so a leaked temp file from a prior invocation that reused
        // this process's pid and counter value collides on `create_new` and,
        // without a retry, fails the open closed — permanently burning the
        // credential on one stale leftover file (VPL-283 AB-283-4).
        let scratch = ScratchCredentials::new("eexist-retry");
        let collider = scratch.dir.join("credentials.json.leftover.tmp");
        let occupant_bytes = b"a leftover file from a prior process, untouched by the retry";

        std::fs::write(&collider, occupant_bytes).expect("seed colliding temp file");

        let collider_name = collider
            .file_name()
            .expect("collider has a file name")
            .to_str()
            .expect("collider name is utf-8")
            .to_string();
        let fresh_name = "credentials.json.fresh.tmp".to_string();

        write_credentials_secure_with_names(
            &scratch.path,
            b"{\"hello\":\"world\"}",
            vec![collider_name, fresh_name].into_iter(),
        )
        .await
        .expect("write must retry past the collision and succeed");

        assert_eq!(
            std::fs::read(&scratch.path).expect("read destination"),
            b"{\"hello\":\"world\"}"
        );

        let mode = std::fs::metadata(&scratch.path)
            .expect("stat credentials")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "the successful retry attempt must still carry mode 0o600"
        );

        // Prohibition (a): unlinking or truncating a name that is already
        // taken is the one way this fix could turn a same-UID nuisance into
        // a destructive primitive against a concurrently-writing process.
        assert_eq!(
            std::fs::read(&collider).expect("read collider"),
            occupant_bytes,
            "the pre-existing colliding file must survive untouched"
        );
    }

    #[tokio::test]
    async fn write_credentials_secure_gives_up_once_every_candidate_name_is_taken() {
        // Bound test pairing AC10: every candidate occupied must return
        // `Err`, not retry forever.
        let scratch = ScratchCredentials::new("eexist-exhausted");
        let collider_a = scratch.dir.join("a.tmp");
        let collider_b = scratch.dir.join("b.tmp");

        std::fs::write(&collider_a, b"a").expect("seed collider a");
        std::fs::write(&collider_b, b"b").expect("seed collider b");

        let names = vec![
            collider_a
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            collider_b
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        ];

        let error = write_credentials_secure_with_names(&scratch.path, b"{}", names.into_iter())
            .await
            .expect_err("every candidate occupied must fail rather than loop")
            .to_string();

        // The count is what tells exhaustion apart from a failure on attempt
        // one — a bare "it errored" cannot, and that is the one distinction
        // this test's name promises.
        assert!(
            error.contains("every one of 2 candidate temp-file names"),
            "the error must report every candidate as tried, got: {}",
            error
        );

        assert!(
            !scratch.path.exists(),
            "an exhausted retry must not write the destination"
        );
        assert_eq!(std::fs::read(&collider_a).unwrap(), b"a");
        assert_eq!(std::fs::read(&collider_b).unwrap(), b"b");
    }

    #[tokio::test]
    async fn write_credentials_secure_reports_an_empty_candidate_list_as_no_attempt() {
        // Exhaustion means every name drawn was taken. A caller that offers
        // no names collided with nothing, and saying "every one of 0 names
        // was already taken" reports a collision that never happened.
        let scratch = ScratchCredentials::new("no-candidates");

        let error =
            write_credentials_secure_with_names(&scratch.path, b"{}", std::iter::empty::<String>())
                .await
                .expect_err("a write with no candidate name cannot succeed")
                .to_string();

        assert!(
            !error.contains("already taken"),
            "no name was drawn, so nothing was taken, got: {}",
            error
        );
        assert!(
            error.contains("no candidate temp-file name was offered"),
            "the error must say no name was offered, got: {}",
            error
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn write_credentials_secure_gives_overlapping_writes_their_own_temp_names() {
        // The shipped composition, not the seam beneath it:
        // `write_credentials_secure` must hand the retry the per-call name
        // generator. A wrapper passing one constant name instead satisfies
        // every test that writes alone into a fresh directory, because a
        // reused name only collides while another write's temp file is still
        // on disk — between its `create_new` and its `rename`.
        //
        // Any timing weakness here fails toward that mutant surviving, never
        // toward a spurious red: with a fresh name per call, every possible
        // interleaving succeeds.
        let scratch = ScratchCredentials::new("overlapping-writes");
        let bytes = vec![b'x'; 512 * 1024];

        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let path = scratch.path.clone();
            let bytes = bytes.clone();

            tasks.spawn(async move { write_credentials_secure(&path, &bytes).await });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("overlapping credential writes must each draw their own temp name");
        }

        assert_eq!(
            std::fs::read(&scratch.path)
                .expect("read destination")
                .len(),
            bytes.len()
        );
        assert!(
            scratch.leftover_temp_files().is_empty(),
            "a committed write leaves no temp file behind"
        );
    }

    #[test]
    fn temp_file_candidate_names_draws_a_fresh_name_for_every_attempt() {
        // The retry is only a retry if each attempt gets a name the last one
        // did not: a generator that hands back the name it just collided on
        // re-collides forever inside the bound and still fails the write
        // closed. This is the production generator `write_credentials_secure`
        // passes to the retry, drawn here directly because the counter it
        // advances is process-global and racing it from a write is not
        // deterministic under the parallel test harness.
        // A bare path: the generator reads only its file name, and spelling a
        // real temp directory here would suggest a filesystem the test never
        // touches.
        let path = Path::new("credentials.json");

        let names: Vec<String> = temp_file_candidate_names(path)
            .take(TEMP_FILE_MAX_ATTEMPTS)
            .collect();

        let distinct: std::collections::BTreeSet<&String> = names.iter().collect();

        assert_eq!(
            distinct.len(),
            names.len(),
            "every candidate name must be distinct, got {:?}",
            names
        );

        for name in &names {
            assert!(
                name.starts_with(&format!("credentials.json.{}.", std::process::id())),
                "a candidate name must stay a bare temp name beside the credentials file, got {}",
                name
            );
            assert!(
                name.ends_with(".tmp"),
                "unexpected candidate name: {}",
                name
            );
        }
    }

    /// A minimal HTTP/1.1 stand-in for an IdP: it answers each request with
    /// whatever `respond` returns for that request's path, or holds the
    /// connection open forever when that is `None`. Only a true external
    /// boundary is faked here — the code under test is the real exchange.
    struct IdpServer {
        addr: std::net::SocketAddr,
        paths: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl IdpServer {
        async fn start(
            respond: impl Fn(&str, std::net::SocketAddr) -> Option<String> + Send + Sync + 'static,
        ) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind idp fixture");
            let addr = listener.local_addr().expect("fixture address");
            let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
            let respond = Arc::new(respond);
            let accepted = paths.clone();

            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        return;
                    };

                    let respond = respond.clone();
                    let accepted = accepted.clone();

                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt;

                        let mut buffer = vec![0u8; 8192];
                        let read = socket.read(&mut buffer).await.unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_string();

                        accepted.lock().unwrap().push(path.clone());

                        match respond(&path, addr) {
                            Some(response) => {
                                let _ = socket.write_all(response.as_bytes()).await;
                            }
                            // Accept and never answer, so the caller's own
                            // timeout is the only thing that ends the request.
                            None => std::future::pending::<()>().await,
                        }
                    });
                }
            });

            Self { addr, paths }
        }

        fn issuer(&self) -> String {
            format!("http://127.0.0.1:{}", self.addr.port())
        }

        fn requested_paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    /// A complete HTTP/1.1 response with a JSON body. `status` is the status
    /// line's code and reason, e.g. `200 OK` or, for a token endpoint
    /// rejecting an invalid or already-consumed refresh token,
    /// `400 Bad Request`.
    fn http_json_status(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        )
    }

    fn http_json(body: &str) -> String {
        http_json_status("200 OK", body)
    }

    fn discovery_document(addr: std::net::SocketAddr) -> String {
        http_json(&format!(
            "{{\"token_endpoint\":\"http://127.0.0.1:{}/token\"}}",
            addr.port()
        ))
    }

    #[tokio::test]
    async fn refresh_access_token_exchanges_against_a_live_token_endpoint() {
        // The real exchange — discovery, the token POST, `expires_in`
        // defaulting, rotation normalization — against a local IdP. None of
        // it was covered by any test before (VPL-183 C15).
        let idp = IdpServer::start(|path, addr| {
            if path.contains(".well-known") {
                return Some(discovery_document(addr));
            }

            // No `expires_in` in the response, which must default to 3600.
            Some(http_json(
                "{\"access_token\":\"fresh-access\",\"token_type\":\"bearer\",\"refresh_token\":\"rotated-by-idp\"}",
            ))
        })
        .await;

        let before = unix_now();

        let (access_token, expires_in, issued_at, rotated) = refresh_access_token(
            Some("aud-1"),
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_secs(5),
        )
        .await
        .map_err(anyhow::Error::from)
        .expect("exchange against the fixture IdP");

        assert_eq!(access_token, "fresh-access");
        assert_eq!(
            expires_in, 3600,
            "a missing expires_in must default to 3600"
        );
        assert_eq!(rotated.as_deref(), Some("rotated-by-idp"));
        assert!(issued_at >= before);
        assert_eq!(
            idp.requested_paths(),
            vec![
                "/.well-known/openid-configuration".to_string(),
                "/token".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn live_token_refresher_performs_the_real_exchange() {
        let idp = IdpServer::start(|path, addr| {
            if path.contains(".well-known") {
                return Some(discovery_document(addr));
            }

            Some(http_json(
                "{\"access_token\":\"fresh-access\",\"token_type\":\"bearer\",\"expires_in\":120}",
            ))
        })
        .await;

        let (access_token, expires_in, _, rotated) = LiveTokenRefresher
            .refresh(None, "client-1", &idp.issuer(), "stored-refresh")
            .await
            .map_err(anyhow::Error::from)
            .expect("live refresher exchange");

        assert_eq!(access_token, "fresh-access");
        assert_eq!(expires_in, 120);
        assert_eq!(
            rotated, None,
            "an IdP that does not rotate must leave the stored token alone"
        );
    }

    #[tokio::test]
    async fn refresh_access_token_gives_up_on_a_hung_idp() {
        // The timeout is the only thing standing between a hung IdP and a
        // permanently stalled refresh, and it defends a failure no other test
        // injects.
        let idp = IdpServer::start(|_, _| None).await;

        let failure = refresh_access_token(
            None,
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_millis(250),
        )
        .await
        .expect_err("a hung IdP must not hang the caller");

        assert!(
            matches!(failure, RefreshFailure::NotSent(_)),
            "a discovery request that never completed never carried the token"
        );
    }

    #[tokio::test]
    async fn refresh_access_token_refuses_a_plaintext_issuer() {
        let failure = refresh_access_token(
            None,
            "client-1",
            "http://idp.example.com",
            "stored-refresh",
            std::time::Duration::from_millis(250),
        )
        .await
        .expect_err("a refresh token must not travel in cleartext");

        let error = anyhow::Error::from(failure).to_string();

        assert!(
            error.contains("must be https"),
            "unexpected error: {}",
            error
        );
    }

    #[tokio::test]
    async fn refresh_access_token_classifies_a_rejected_token_post_as_sent() {
        // AC9 (VPL-283 C-6): the Sent/NotSent split at the token-POST call
        // site (`.map_err(|e| RefreshFailure::Sent(...))`) is what the memo
        // depends on to decide whether a failed exchange is safe to retry.
        // Every "replay" test elsewhere in this module builds a `Sent`
        // verdict by hand via `ScriptedRefresher`; none exercises the real
        // classification, so mutating that single arm to `NotSent` left the
        // full suite green (VPL183-T9). Drive the real POST through a
        // fixture that rejects it instead.
        let idp = IdpServer::start(|path, addr| {
            if path.contains(".well-known") {
                return Some(discovery_document(addr));
            }

            Some(http_json_status(
                "400 Bad Request",
                "{\"error\":\"invalid_grant\"}",
            ))
        })
        .await;

        let failure = refresh_access_token(
            None,
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_secs(5),
        )
        .await
        .expect_err("a rejected token POST must be classified as an error");

        assert!(
            matches!(failure, RefreshFailure::Sent(_)),
            "a rejected token POST must be classified Sent: the IdP may already have consumed the token"
        );

        // Positive control discriminating against AB-283-6: without this,
        // the assertion above cannot tell "classified Sent by the real POST"
        // from "rejected as NotSent at the origin check before the POST was
        // ever made" — both produce an error, but only one exercises the
        // mutant's line. Containment is the whole property; pinning the exact
        // request sequence would additionally fail on a discovery retry or a
        // keep-alive reconnect that changes nothing about it.
        assert!(
            idp.requested_paths().iter().any(|path| path == "/token"),
            "the token POST must actually have been attempted, saw {:?}",
            idp.requested_paths()
        );
    }

    #[tokio::test]
    async fn refresh_access_token_classifies_a_timed_out_token_post_as_sent() {
        // The `Sent` comment at the token POST claims a transport error, a
        // timeout and a rejection are indistinguishable once the token is on
        // the wire. Only rejection was ever injected; a timeout is the shape
        // a degraded IdP produces most often, and it is the one where "the
        // IdP may already have consumed it" is least obvious. Serve discovery
        // normally, then accept the token POST and never answer it.
        let idp = IdpServer::start(|path, addr| {
            if path.contains(".well-known") {
                return Some(discovery_document(addr));
            }

            None
        })
        .await;

        let failure = refresh_access_token(
            None,
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_millis(250),
        )
        .await
        .expect_err("a token POST that never completes must not hang the caller");

        assert!(
            matches!(failure, RefreshFailure::Sent(_)),
            "a token POST that timed out must be classified Sent: the IdP has the token whether or not it answered"
        );

        assert!(
            idp.requested_paths().iter().any(|path| path == "/token"),
            "the timeout must have happened at the token POST, saw {:?}",
            idp.requested_paths()
        );
    }

    #[tokio::test]
    async fn refresh_access_token_refuses_a_token_endpoint_on_another_origin() {
        // The token endpoint is named by remote data. A tampered discovery
        // document must not be able to relocate the refresh token to a host
        // the user never logged in to (VPL-183 C16).
        let idp = IdpServer::start(|path, _| {
            if path.contains(".well-known") {
                return Some(http_json(
                    "{\"token_endpoint\":\"https://attacker.example.com/token\"}",
                ));
            }

            Some(http_json("{}"))
        })
        .await;

        let failure = refresh_access_token(
            None,
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_millis(250),
        )
        .await
        .expect_err("a cross-origin token endpoint must be refused");

        assert!(
            matches!(failure, RefreshFailure::NotSent(_)),
            "the token is refused before it is sent, so it stays usable"
        );
        assert_eq!(
            idp.requested_paths(),
            vec!["/.well-known/openid-configuration".to_string()],
            "no token request may be issued once the endpoint is rejected"
        );
    }

    #[tokio::test]
    async fn refresh_access_token_does_not_follow_a_redirected_token_endpoint() {
        // A 307 preserves the method and the body, so following one would
        // replay the credential-bearing POST wherever the redirector points.
        let elsewhere = IdpServer::start(|_, _| Some(http_json("{}"))).await;
        let elsewhere_port = elsewhere.addr.port();

        let idp = IdpServer::start(move |path, addr| {
            if path.contains(".well-known") {
                return Some(discovery_document(addr));
            }

            Some(format!(
                "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://127.0.0.1:{}/token\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                elsewhere_port
            ))
        })
        .await;

        refresh_access_token(
            None,
            "client-1",
            &idp.issuer(),
            "stored-refresh",
            std::time::Duration::from_secs(5),
        )
        .await
        .expect_err("a redirected token exchange must fail rather than be followed");

        assert!(
            elsewhere.requested_paths().is_empty(),
            "the refresh token must not be replayed to the redirect target"
        );
    }
}
