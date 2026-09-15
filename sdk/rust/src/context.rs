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
    collections::{BTreeMap, HashMap},
    net::{Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{read, OpenOptions},
    io::AsyncWriteExt,
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
    // Discover token endpoint
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let doc: serde_json::Value = reqwest::get(&discovery_url).await?.json().await?;

    let token_endpoint = doc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing token_endpoint in OIDC discovery"))?;

    // Create OAuth2 client
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(AuthUrl::new(issuer.to_string())?)
        .set_token_uri(TokenUrl::new(token_endpoint.to_string())?);

    // Exchange refresh token
    let http_client = reqwest::Client::new();
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

/// Writes credential bytes to `path` enforcing mode 0o600 on file create.
///
/// `OpenOptions::mode()` only takes effect when the file is created — if the
/// file already exists, the existing mode is preserved. Both Rust call sites
/// for `credentials.json` (login at `cli/src/command.rs` and refresh here)
/// must use this pattern so the file is born 0o600 and not 0o644 (umask 022).
async fn write_credentials_secure(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    Ok(())
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
    let credentials_path = get_key_credentials_path();

    if !credentials_path.exists() {
        return Ok(None);
    }

    let credentials_data = read(&credentials_path).await?;
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

    let token_age = now - issuer_creds.issued_at;

    // Refresh if token has less than 5 minutes left
    let needs_refresh = token_age + 300 >= issuer_creds.expires_in;

    if needs_refresh {
        // Skip refresh if no refresh token available (user must re-login)
        if issuer_creds.refresh_token.is_empty() {
            return Err(anyhow!(
                "Access token expired and no refresh token available. Please run: vorpal login --issuer {registry_issuer}"
            ));
        }

        let (new_token, new_expires, new_issued_at, rotated_refresh) = refresh_access_token(
            issuer_creds.audience.as_deref(),
            &issuer_creds.client_id,
            registry_issuer,
            &issuer_creds.refresh_token,
        )
        .await?;

        // Now update the credentials
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

        // Save updated credentials with mode 0o600 enforced on create.
        let credentials_json = serde_json::to_string_pretty(&credentials)?;
        write_credentials_secure(&credentials_path, credentials_json.as_bytes()).await?;
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
}
