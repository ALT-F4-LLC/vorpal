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
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    fs::{read, rename, OpenOptions},
    io::AsyncWriteExt,
    sync::Mutex,
};
use tonic::{
    metadata::{Ascii, MetadataValue},
    transport::{Certificate, Channel, ClientTlsConfig, Server},
    Code::NotFound,
    Request, Response, Status,
};
use tracing::info;

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

/// Refreshes an expired access token using the refresh token.
///
/// Returns `(access_token, expires_in, issued_at, rotated_refresh_token)`.
/// `rotated_refresh_token` is `Some(new)` when the `IdP` rotated the refresh
/// token (Zitadel default), `None` when it did not (caller should keep the
/// existing refresh token).
async fn refresh_access_token(
    audience: Option<&str>,
    client_id: &str,
    issuer: &str,
    refresh_token: &str,
) -> Result<(String, u64, u64, Option<String>)> {
    // Bounded so a hung IdP stalls only this exchange rather than every
    // caller queued behind CREDENTIALS_REFRESH_LOCK indefinitely (neither
    // call below had a timeout before, which was tolerable when each caller
    // hung independently; serializing them makes an unbounded hang total).
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Discover token endpoint
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let doc: serde_json::Value = http_client.get(&discovery_url).send().await?.json().await?;

    let token_endpoint = doc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing token_endpoint in OIDC discovery"))?;

    // Create OAuth2 client
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(AuthUrl::new(issuer.to_string())?)
        .set_token_uri(TokenUrl::new(token_endpoint.to_string())?);

    // Exchange refresh token
    let refresh_token_obj = RefreshToken::new(refresh_token.to_string());
    let mut request = client.exchange_refresh_token(&refresh_token_obj);

    // Only add audience if provided (Auth0 requires it, others may not)
    if let Some(aud) = audience {
        request = request.add_extra_param("audience", aud);
    }

    let token_result = request.request_async(&http_client).await?;

    let new_access_token = token_result.access_token().secret().clone();
    let new_expires_in = token_result.expires_in().map_or(3600, |d| d.as_secs());
    let new_refresh_token =
        normalize_rotated_refresh_token(token_result.refresh_token().map(|t| t.secret().clone()));

    let issued_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

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

/// Unlinks the temp file at `path` when dropped, unless [`disarm`] was
/// called first. Guards `write_credentials_secure`'s temp file from the
/// moment its path is chosen through to a committed `rename`, so a
/// cancelled task, a dropped `JoinSet`, or a panic still removes a file
/// that may hold a live plaintext refresh token (VPL-183 AB-3) — the old
/// `if write_result.is_err()` cleanup ran only after the write future was
/// fully awaited, so none of those exits ever reached it.
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

    /// Marks the temp file as committed (renamed onto its destination) so
    /// `Drop` leaves it alone.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
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
/// [`TempFileGuard`] covers the temp file for every exit path from this
/// function — success disarms it after `rename` commits; any early return,
/// cancellation, or panic leaves it armed and `Drop` unlinks the file.
async fn write_credentials_secure(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "credentials path has no parent directory: {}",
            path.display()
        )
    })?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("credentials");

    let temp_path = parent.join(format!(
        "{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let mut guard = TempFileGuard::new(temp_path.clone());

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .await?;

    file.write_all(bytes).await?;
    // fsync, not flush: flush is a userspace buffer flush, and a rename
    // ordered before the data reaches disk can leave a zero-length
    // credentials.json after a crash.
    file.sync_all().await?;

    rename(&temp_path, path).await?;

    guard.disarm();

    Ok(())
}

/// Performs the OAuth refresh-token exchange. Injected as a [`TokenRefresher`]
/// so tests can count and control exchanges without real network I/O.
///
/// Contract: `refresh` runs while `CREDENTIALS_REFRESH_LOCK` is held (see
/// its doc comment) and must not call back into `client_auth_header` or
/// `client_auth_header_at` — that lock is a non-reentrant
/// `tokio::sync::Mutex`, and a re-entrant call deadlocks every authenticated
/// RPC in the process.
#[tonic::async_trait]
trait TokenRefresher: Send + Sync {
    async fn refresh(
        &self,
        audience: Option<&str>,
        client_id: &str,
        issuer: &str,
        refresh_token: &str,
    ) -> Result<(String, u64, u64, Option<String>)>;
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
    ) -> Result<(String, u64, u64, Option<String>)> {
        refresh_access_token(audience, client_id, issuer, refresh_token).await
    }
}

/// Serializes credential read, refresh-decision, refresh exchange and file
/// write within this process. Held for the whole critical section, not just
/// the write, so a waiter re-reads the winner's post-refresh state instead
/// of retrying a decision it made against a stale snapshot.
///
/// Scope is this process only: it does not serialize against a separately
/// spawned config process, a running `vorpal start agent`, or the Go/
/// TypeScript SDKs writing the same `credentials.json`. `write_credentials_secure`'s
/// atomic write independently keeps every one of those readers from ever
/// observing a torn file; only the refresh-exchange race is process-local.
static CREDENTIALS_REFRESH_LOCK: Mutex<()> = Mutex::const_new(());

/// Process-lifetime record of refresh-token *values* whose exchange has
/// already failed, keyed by a SHA-256 digest of the token value — never the
/// plaintext, and never logged. A failed or timed-out exchange leaves
/// `credentials.json` byte-identical, so without this memo every waiter
/// serialized behind `CREDENTIALS_REFRESH_LOCK` re-reads the same file,
/// re-decides "needs refresh", and replays the same one-time refresh token
/// against the IdP (VPL-183 AB-1) — the lock serializes that replay, it does
/// not prevent it.
///
/// Failure here is terminal for that token *value*, not a backoff: a
/// time-based retry would still replay the same already-consumed token once
/// the timer expired, which is the same hazard with a delay. Keying on the
/// value (not the issuer) matters too — a legitimately rotated new token has
/// a different digest and is unaffected by an older value's terminal
/// failure.
///
/// Scope matches `CREDENTIALS_REFRESH_LOCK`: process-lifetime only. It is
/// not durable, and is not visible to a separately spawned process or to
/// the Go/TypeScript SDKs writing the same file.
static FAILED_REFRESH_TOKENS: Mutex<BTreeSet<String>> = Mutex::const_new(BTreeSet::new());

/// Core of [`client_auth_header`], taking the credentials path and the
/// refresh operation as parameters so it is testable without touching the
/// real `/var/lib/vorpal/key/credentials.json` or performing network I/O.
/// Deliberately private and not configurable from any production entry
/// point (env var, global override) — see `client_auth_header`.
async fn client_auth_header_at(
    credentials_path: &Path,
    registry: &str,
    refresher: &dyn TokenRefresher,
) -> Result<Option<MetadataValue<Ascii>>> {
    // Acquired before the existence check and the read below, and held
    // through the write: see CREDENTIALS_REFRESH_LOCK's doc comment.
    let _guard = CREDENTIALS_REFRESH_LOCK.lock().await;

    if !credentials_path.exists() {
        return Ok(None);
    }

    let credentials_data = read(credentials_path).await?;
    let mut credentials: VorpalCredentials = serde_json::from_slice(&credentials_data)?;

    // Borrowed from `credentials.registry`; the refresh below only mutates
    // `credentials.issuer`, so the borrow stays valid across it.
    let Some(registry_issuer) = credentials.registry.get(registry) else {
        return Ok(None);
    };

    // Check if token needs refresh
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let issuer_creds = credentials
        .issuer
        .get(registry_issuer)
        .ok_or_else(|| anyhow!("no credentials for issuer: {registry_issuer}"))?;

    let needs_refresh = {
        let expires_in = issuer_creds.expires_in;

        if issuer_creds.issued_at > now {
            // `issued_at` is file-sourced and unvalidated (VPL-183 AB-5). A
            // future-dated value means the age is unknown, not zero: the
            // previous `saturating_sub`-only computation clamped this case
            // to age 0, which made the token look freshly issued and
            // suppressed a refresh that may genuinely be due — the opposite
            // of fail-safe. Unknown age must fail toward refreshing.
            true
        } else {
            let token_age = now - issuer_creds.issued_at;

            // Refresh once fewer than `refresh_window` seconds remain.
            // 300s (5 minutes) is the normal window, but that fixed
            // constant makes this check unconditionally true the instant an
            // IdP hands out `expires_in <= 300`, so every waiter decides
            // independently "needs refresh" against a token that was only
            // just issued (VPL-183 AB-4, a self-amplifying rotation storm).
            // For a token that short-lived, use half its lifetime instead,
            // so a token issued `now` does not immediately re-qualify.
            let refresh_window = if expires_in <= 300 {
                expires_in / 2
            } else {
                300
            };

            token_age + refresh_window >= expires_in
        }
    };

    if needs_refresh {
        // Skip refresh if no refresh token available (user must re-login)
        if issuer_creds.refresh_token.is_empty() {
            return Err(anyhow!(
                "Access token expired and no refresh token available. Please run: vorpal login --issuer {registry_issuer}"
            ));
        }

        // See FAILED_REFRESH_TOKENS: a prior waiter may already have burned
        // this exact stored token value in a failed exchange. Re-reading the
        // byte-identical post-failure file cannot tell that apart from a
        // token that has simply never been tried, so the memo is what does.
        let refresh_token_digest = digest(issuer_creds.refresh_token.as_str());
        if FAILED_REFRESH_TOKENS
            .lock()
            .await
            .contains(&refresh_token_digest)
        {
            return Err(anyhow!(
                "OAuth refresh-token exchange already failed for the stored token. Please run: vorpal login --issuer {}",
                registry_issuer
            ));
        }

        let refresh_result = refresher
            .refresh(
                issuer_creds.audience.as_deref(),
                &issuer_creds.client_id,
                registry_issuer,
                &issuer_creds.refresh_token,
            )
            .await;

        let (new_token, new_expires, new_issued_at, rotated_refresh) = match refresh_result {
            Ok(outcome) => outcome,
            Err(err) => {
                // Terminal for this token value, never a backoff: see
                // FAILED_REFRESH_TOKENS's doc comment for why a retry
                // window still replays a consumed token.
                FAILED_REFRESH_TOKENS
                    .lock()
                    .await
                    .insert(refresh_token_digest);
                return Err(err);
            }
        };

        // Now update the credentials. No await is introduced between the
        // exchange above and the write below: the sequence stays
        // synchronous so the window in which a killed task loses the
        // rotated token to disk (accepted residual risk) does not widen.
        let issuer_creds = credentials
            .issuer
            .get_mut(registry_issuer)
            .ok_or_else(|| anyhow!("no credentials for issuer: {registry_issuer}"))?;

        apply_token_refresh(
            issuer_creds,
            new_token,
            new_expires,
            new_issued_at,
            rotated_refresh,
        );

        // Save updated credentials with mode 0o600 enforced on the temp file.
        let credentials_json = serde_json::to_string_pretty(&credentials)?;
        write_credentials_secure(credentials_path, credentials_json.as_bytes()).await?;
    }

    // Get the access token
    let access_token = &credentials
        .issuer
        .get(registry_issuer)
        .ok_or_else(|| anyhow!("no credentials for issuer: {registry_issuer}"))?
        .access_token;

    let header = format!("Bearer {access_token}")
        .parse()
        .map_err(|e| anyhow!("failed to parse Bearer token: {e}"))?;

    Ok(Some(header))
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
    client_auth_header_at(&get_key_credentials_path(), registry, &LiveTokenRefresher).await
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-mode-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("credentials.json");
        // Sanity: the path must not pre-exist — we are testing file birth, not
        // an inherited mode from a pre-created 0o600 file.
        assert!(!path.exists(), "test path must be previously-nonexistent");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(write_credentials_secure(&path, b"{\"hello\":\"world\"}"))?;

        let mode = std::fs::metadata(&path)?.permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credentials file must be born 0o600, got {:o}",
            mode & 0o777
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);

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
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-mode-regression-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        // Pre-create the destination at 0o644, the mode a naive
        // `File::create`-based temp writer would carry onto the destination
        // via `rename` (A-4). The fix must not inherit it.
        std::fs::write(&path, b"{\"stale\":true}").expect("write pre-existing file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("set pre-existing mode to 0o644");

        write_credentials_secure(&path, b"{\"hello\":\"world\"}")
            .await
            .expect("write credentials");

        let mode = std::fs::metadata(&path)
            .expect("stat credentials")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credentials file must be 0o600 after overwriting a pre-existing 0o644 file, got {:o}",
            mode & 0o777
        );

        let contents = std::fs::read(&path).expect("read credentials");
        assert_eq!(contents, b"{\"hello\":\"world\"}");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn client_auth_header_at_serializes_concurrent_refresh() {
        // AC1 + AC2: N concurrent callers deciding "refresh needed" against
        // the same credentials file must trigger exactly one refresh
        // exchange, and the file left behind must hold the single winner's
        // rotated token — not a stale snapshot written by a waiter who
        // decided "refresh needed" before the guard (A-2).
        struct CountingRefresher {
            calls: std::sync::atomic::AtomicU32,
        }

        #[tonic::async_trait]
        impl TokenRefresher for CountingRefresher {
            async fn refresh(
                &self,
                _audience: Option<&str>,
                _client_id: &str,
                _issuer: &str,
                _refresh_token: &str,
            ) -> Result<(String, u64, u64, Option<String>)> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Ok((
                    "rotated-access".to_string(),
                    3600,
                    now,
                    Some("rotated-refresh-once".to_string()),
                ))
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-concurrency-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token 4 minutes from expiry: token_age + 300 >= expires_in.
        let expires_in = 3600u64;
        let issued_at = now - (expires_in - 240);

        let mut issuer = BTreeMap::new();
        issuer.insert(
            "issuer-1".to_string(),
            VorpalCredentialsContent {
                access_token: "old-access".to_string(),
                audience: None,
                client_id: "client-1".to_string(),
                expires_in,
                issued_at,
                refresh_token: "old-refresh".to_string(),
                scopes: vec!["openid".to_string()],
            },
        );
        let mut registry = BTreeMap::new();
        registry.insert("registry-1".to_string(), "issuer-1".to_string());

        let credentials = VorpalCredentials { issuer, registry };
        std::fs::write(&path, serde_json::to_vec(&credentials).unwrap()).expect("write fixture");

        let refresher = std::sync::Arc::new(CountingRefresher {
            calls: std::sync::atomic::AtomicU32::new(0),
        });

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let path = path.clone();
            let refresher = refresher.clone();
            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref()).await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("client_auth_header_at failed");
        }

        assert_eq!(
            refresher.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one refresh exchange must occur for N concurrent callers"
        );

        let final_bytes = std::fs::read(&path).expect("read final credentials");
        let final_credentials: VorpalCredentials =
            serde_json::from_slice(&final_bytes).expect("parse final credentials");
        let final_issuer_creds = final_credentials
            .issuer
            .get("issuer-1")
            .expect("issuer present");

        assert_eq!(
            final_issuer_creds.refresh_token, "rotated-refresh-once",
            "stored refresh token must be the single winner's rotated token, not a stale replay"
        );
        assert_eq!(final_issuer_creds.access_token, "rotated-access");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
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
        struct CountingRefresher {
            calls: std::sync::atomic::AtomicU32,
        }

        #[tonic::async_trait]
        impl TokenRefresher for CountingRefresher {
            async fn refresh(
                &self,
                _audience: Option<&str>,
                _client_id: &str,
                _issuer: &str,
                _refresh_token: &str,
            ) -> Result<(String, u64, u64, Option<String>)> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Ok((
                    "rotated-access".to_string(),
                    3600,
                    now,
                    Some("rotated-refresh-skew".to_string()),
                ))
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-future-issued-at-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut issuer = BTreeMap::new();
        issuer.insert(
            "issuer-1".to_string(),
            VorpalCredentialsContent {
                access_token: "old-access".to_string(),
                audience: None,
                client_id: "client-1".to_string(),
                expires_in: 3600,
                issued_at: now + 3600, // future-dated, e.g. clock skew
                refresh_token: "old-refresh-skew".to_string(),
                scopes: vec!["openid".to_string()],
            },
        );
        let mut registry = BTreeMap::new();
        registry.insert("registry-1".to_string(), "issuer-1".to_string());

        let credentials = VorpalCredentials { issuer, registry };
        std::fs::write(&path, serde_json::to_vec(&credentials).unwrap()).expect("write fixture");

        let refresher = CountingRefresher {
            calls: std::sync::atomic::AtomicU32::new(0),
        };

        let header = client_auth_header_at(&path, "registry-1", &refresher)
            .await
            .expect("must not panic or error on a future-dated issued_at");

        assert!(
            header.is_some(),
            "a refresh attempted for a skewed token must still yield an auth header"
        );
        assert_eq!(
            refresher.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a future-dated issued_at must not suppress a refresh"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
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
        struct FailingRefresher {
            calls: std::sync::atomic::AtomicU32,
        }

        #[tonic::async_trait]
        impl TokenRefresher for FailingRefresher {
            async fn refresh(
                &self,
                _audience: Option<&str>,
                _client_id: &str,
                _issuer: &str,
                _refresh_token: &str,
            ) -> Result<(String, u64, u64, Option<String>)> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(anyhow!("simulated IdP timeout"))
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-failed-exchange-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token 4 minutes from expiry: token_age + 300 >= expires_in.
        let expires_in = 3600u64;
        let issued_at = now - (expires_in - 240);

        let mut issuer = BTreeMap::new();
        issuer.insert(
            "issuer-1".to_string(),
            VorpalCredentialsContent {
                access_token: "old-access".to_string(),
                audience: None,
                client_id: "client-1".to_string(),
                expires_in,
                issued_at,
                refresh_token: "old-refresh-failing".to_string(),
                scopes: vec!["openid".to_string()],
            },
        );
        let mut registry = BTreeMap::new();
        registry.insert("registry-1".to_string(), "issuer-1".to_string());

        let credentials = VorpalCredentials { issuer, registry };
        std::fs::write(&path, serde_json::to_vec(&credentials).unwrap()).expect("write fixture");

        let refresher = std::sync::Arc::new(FailingRefresher {
            calls: std::sync::atomic::AtomicU32::new(0),
        });

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let path = path.clone();
            let refresher = refresher.clone();
            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref()).await
            });
        }

        let mut error_count = 0;
        while let Some(result) = tasks.join_next().await {
            let outcome = result.expect("task panicked");
            assert!(
                outcome.is_err(),
                "every caller must observe the failure, not a stale success"
            );
            error_count += 1;
        }
        assert_eq!(error_count, 8);

        assert_eq!(
            refresher.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one exchange attempt must occur even though 8 callers observed a failure"
        );

        let final_bytes = std::fs::read(&path).expect("read final credentials");
        let final_credentials: VorpalCredentials =
            serde_json::from_slice(&final_bytes).expect("parse final credentials");
        let final_issuer_creds = final_credentials
            .issuer
            .get("issuer-1")
            .expect("issuer present");

        assert_eq!(
            final_issuer_creds.refresh_token, "old-refresh-failing",
            "the stored refresh token must be untouched after a failed exchange"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
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
        struct CountingRefresher {
            calls: std::sync::atomic::AtomicU32,
        }

        #[tonic::async_trait]
        impl TokenRefresher for CountingRefresher {
            async fn refresh(
                &self,
                _audience: Option<&str>,
                _client_id: &str,
                _issuer: &str,
                _refresh_token: &str,
            ) -> Result<(String, u64, u64, Option<String>)> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Ok((
                    "rotated-access".to_string(),
                    60,
                    now,
                    Some("rotated-refresh-short-expiry".to_string()),
                ))
            }
        }

        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-short-expiry-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expires_in = 60u64;
        let issued_at = now - 3600; // well outside any window, regardless of size

        let mut issuer = BTreeMap::new();
        issuer.insert(
            "issuer-1".to_string(),
            VorpalCredentialsContent {
                access_token: "old-access".to_string(),
                audience: None,
                client_id: "client-1".to_string(),
                expires_in,
                issued_at,
                refresh_token: "old-refresh-short-expiry".to_string(),
                scopes: vec!["openid".to_string()],
            },
        );
        let mut registry = BTreeMap::new();
        registry.insert("registry-1".to_string(), "issuer-1".to_string());

        let credentials = VorpalCredentials { issuer, registry };
        std::fs::write(&path, serde_json::to_vec(&credentials).unwrap()).expect("write fixture");

        let refresher = std::sync::Arc::new(CountingRefresher {
            calls: std::sync::atomic::AtomicU32::new(0),
        });

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let path = path.clone();
            let refresher = refresher.clone();
            tasks.spawn(async move {
                client_auth_header_at(&path, "registry-1", refresher.as_ref()).await
            });
        }
        while let Some(result) = tasks.join_next().await {
            result
                .expect("task panicked")
                .expect("client_auth_header_at failed");
        }

        assert_eq!(
            refresher.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exchange count must not scale with the number of concurrent callers for a short-lived token"
        );

        // A second sequential call right after the successful refresh must
        // not immediately rotate again: the freshly issued token
        // (issued_at ~ now, expires_in 60) must fall outside the
        // proportional refresh window.
        client_auth_header_at(&path, "registry-1", refresher.as_ref())
            .await
            .expect("second call must succeed");

        assert_eq!(
            refresher.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a freshly rotated short-lived token must not be rotated again immediately"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
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
        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-torn-read-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("credentials.json");

        const CONTENT_LEN: usize = 8192;
        const ITERATIONS: usize = 150;
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

        assert!(reads > 0, "reader must have observed at least one read");
        assert_eq!(
            torn, 0,
            "reader observed {torn} torn/truncated reads out of {reads}"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn temp_file_guard_removes_the_file_when_dropped_still_armed() {
        // AC5 (VPL-183 C-4), the direct unit-level check on the mechanism:
        // deleting TempFileGuard's Drop impl (mutant M7's modern
        // equivalent, now that cleanup lives in the guard rather than an
        // `is_err` branch) makes this fail. An armed guard going out of
        // scope is exactly what happens on a cancelled task, a dropped
        // `JoinSet`, or a panic — none of those reach a post-await cleanup
        // branch, but all of them run destructors.
        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-guard-armed-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let temp_path = dir.join("credentials.json.12345.0.tmp");
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

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn temp_file_guard_leaves_the_file_when_disarmed() {
        // Companion to the test above: a guard disarmed after a committed
        // rename must not touch the (now-unrelated) path on drop.
        let dir = std::env::temp_dir().join(format!(
            "vorpal-creds-guard-disarmed-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let temp_path = dir.join("credentials.json.12345.1.tmp");
        std::fs::write(&temp_path, b"already-renamed-elsewhere").expect("seed temp file");

        {
            let mut guard = TempFileGuard::new(temp_path.clone());
            guard.disarm();
        }

        assert!(
            temp_path.exists(),
            "a disarmed guard must not touch the file (it already committed via rename)"
        );

        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
