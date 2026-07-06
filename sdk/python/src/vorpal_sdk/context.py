"""ConfigContext, the cross-SDK digest-parity serializer, and the auth/TLS flow.

Mirrors ``sdk/typescript/src/context.ts`` (the canonical reference) module-for-
module in a synchronous model: the digest-parity serializer, OIDC bearer-token
auth with refresh-token rotation, TLS credential selection by URI scheme, the
``ConfigContext`` lifecycle (artifact add/fetch + digest cache), and the
``ContextService`` gRPC server that runs until SIGINT/SIGTERM.

The serializer reproduces ``serializeArtifact``/``computeArtifactDigest``
byte-for-byte so the SHA-256 artifact digest is identical across the
Rust/Go/TS/Python SDKs. Any deviation breaks the cross-SDK parity invariant;
see ``tests/test_parity.py``.

Security (mirrors the TS reference exactly):
- Access/refresh tokens, the Bearer header, and step-secret values are NEVER
  logged or echoed into error text.
- A malformed credentials file fails closed WITHOUT echoing its contents.
- TLS is never downgraded to insecure for a non-local scheme; a missing CA
  falls back to system-root TLS, not plaintext.
- The credentials file is rewritten with mode ``0o600`` after a token refresh.
"""

from __future__ import annotations

import hashlib
import json
import os
import signal
import ssl
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent import futures
from dataclasses import dataclass, field
from typing import Any

import grpc

from vorpal_sdk.api.agent import agent_pb2, agent_pb2_grpc
from vorpal_sdk.api.artifact import artifact_pb2, artifact_pb2_grpc
from vorpal_sdk.api.context import context_pb2_grpc
from vorpal_sdk.cli import parse_cli_args
from vorpal_sdk.system import get_system

# ---------------------------------------------------------------------------
# Custom JSON serialization for cross-SDK parity
# ---------------------------------------------------------------------------
#
# Mirrors Rust's ``serde_json::to_vec`` output for prost structs: field names are
# the proto field names, key order follows proto field-number order, ALL fields
# are always emitted (zero-values and empty repeated fields included), enums
# serialize as integers, and ``optional``-absent serializes as ``null``.
#
# Insertion order of the dicts below IS the proto field-number order — do not
# reorder keys. The dicts are emitted compactly (no whitespace) with raw UTF-8.


def serialize_artifact_step_secret(
    secret: artifact_pb2.ArtifactStepSecret,
) -> dict[str, Any]:
    return {
        "name": secret.name,
        "value": secret.value,
    }


def serialize_artifact_source(source: artifact_pb2.ArtifactSource) -> dict[str, Any]:
    return {
        # ``digest`` is a proto3 ``optional`` — gate on HasField, NOT truthiness:
        # a present-but-empty ``digest=""`` must emit ``""``, an absent one ``null``.
        "digest": source.digest if source.HasField("digest") else None,
        "excludes": list(source.excludes),
        "includes": list(source.includes),
        "name": source.name,
        "path": source.path,
    }


def serialize_artifact_step(step: artifact_pb2.ArtifactStep) -> dict[str, Any]:
    return {
        # entrypoint/script are proto3 optional: HasField, not truthiness.
        "entrypoint": step.entrypoint if step.HasField("entrypoint") else None,
        "script": step.script if step.HasField("script") else None,
        "secrets": [serialize_artifact_step_secret(s) for s in step.secrets],
        "arguments": list(step.arguments),
        "artifacts": list(step.artifacts),
        "environments": list(step.environments),
    }


def serialize_artifact(artifact: artifact_pb2.Artifact) -> dict[str, Any]:
    return {
        # Enums serialize as their integer value (proto enum values are ints).
        "target": int(artifact.target),
        "sources": [serialize_artifact_source(s) for s in artifact.sources],
        "steps": [serialize_artifact_step(s) for s in artifact.steps],
        "systems": [int(s) for s in artifact.systems],
        "aliases": list(artifact.aliases),
        "name": artifact.name,
    }


def artifact_to_json_bytes(artifact: artifact_pb2.Artifact) -> bytes:
    """Serialize an Artifact to the cross-SDK-canonical JSON bytes.

    ``separators`` forces compact output (Python defaults to ``", "``/``": "``)
    and ``ensure_ascii=False`` emits raw UTF-8 (Python defaults to ``True``);
    both defaults would diverge from ``JSON.stringify`` and break digest parity.
    """
    obj = serialize_artifact(artifact)
    return json.dumps(obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def compute_artifact_digest(artifact: artifact_pb2.Artifact) -> str:
    """SHA-256 hex digest of an Artifact, identical across all Vorpal SDKs."""
    return hashlib.sha256(artifact_to_json_bytes(artifact)).hexdigest()


# ---------------------------------------------------------------------------
# TLS credential helper — matches Rust get_client_tls_config() / context.ts:38-49
# ---------------------------------------------------------------------------

VORPAL_ROOT_DIR = "/var/lib/vorpal"
VORPAL_CA_PATH = os.path.join(VORPAL_ROOT_DIR, "key", "ca.pem")
VORPAL_CREDENTIALS_PATH = os.path.join(VORPAL_ROOT_DIR, "key", "credentials.json")


def get_client_credentials(uri: str) -> grpc.ChannelCredentials | None:
    """Select channel credentials for ``uri``, mirroring ``context.ts:38-49``.

    Returns ``None`` ONLY for the local ``http://``/``unix://`` schemes (an
    insecure channel). For every other (non-local) scheme this returns TLS
    credentials and NEVER ``None`` — a pinned-CA channel when the CA file exists,
    otherwise system-root TLS. There is no insecure downgrade for a non-local
    scheme even when the CA is absent.
    """
    if uri.startswith("http://") or uri.startswith("unix://"):
        return None

    if os.path.exists(VORPAL_CA_PATH):
        with open(VORPAL_CA_PATH, "rb") as f:
            ca_pem = f.read()
        return grpc.ssl_channel_credentials(root_certificates=ca_pem)

    # CA absent → system-root TLS (grpcio uses its bundled/default roots), NOT
    # an insecure channel.
    return grpc.ssl_channel_credentials()


def to_grpc_target(uri: str) -> str:
    """Convert a URI to a grpcio-compatible target string.

    grpcio expects ``host:port`` or ``unix:///path``, not ``https://``/``http://``
    schemes. The Rust SDK's tonic accepts ``https://`` natively, so the CLI passes
    registry/agent URLs in that form; we normalize here (mirrors ``context.ts:60-73``).
    """
    if uri.startswith("unix://"):
        return uri
    if uri.startswith("https://"):
        host = uri[len("https://") :].rstrip("/")
        return host if ":" in host else f"{host}:443"
    if uri.startswith("http://"):
        host = uri[len("http://") :].rstrip("/")
        return host if ":" in host else f"{host}:80"
    return uri


def _channel_options(
    target: str, credentials: grpc.ChannelCredentials | None
) -> list[tuple[str, str]]:
    """grpcio channel options for ``target``.

    For an insecure unix-socket channel grpcio derives the HTTP/2 ``:authority``
    from the socket path, percent-encoded (e.g. ``tmp%2Fvorpal.sock``), which a
    strict h2 server (tonic) rejects as a malformed authority. Pin a valid
    authority so the request is accepted; TLS/TCP targets keep their host-derived
    authority (needed for SNI).

    The override is gated on ``credentials is None`` (an insecure channel), so a
    TLS/secure channel can NEVER receive the localhost authority override — even
    for a non-canonical ``unix:relative`` target that matches the loose ``unix:``
    prefix but selects TLS credentials (via ``get_client_credentials``, which
    keys on ``unix://``). Keying on the credential scheme rather than the target
    prefix alone eliminates the string-matching ambiguity between the two checks.
    """
    if credentials is None and target.startswith("unix:"):
        return [("grpc.default_authority", "localhost")]
    return []


def _create_channel(uri: str) -> grpc.Channel:
    target = to_grpc_target(uri)
    credentials = get_client_credentials(uri)
    options = _channel_options(target, credentials)
    if credentials is None:
        return grpc.insecure_channel(target, options=options)
    return grpc.secure_channel(target, credentials, options=options)


# ---------------------------------------------------------------------------
# OIDC auth header — matches Rust client_auth_header() / Go ClientAuthHeader()
# ---------------------------------------------------------------------------
#
# Security: access/refresh tokens and the Bearer header are NEVER logged or
# placed in error text. The credentials file is fail-closed on parse errors and
# rewritten with mode 0o600 after a refresh.


@dataclass
class _RefreshResult:
    access_token: str
    expires_in: int
    issued_at: int
    refresh_token: str | None


def refresh_access_token(
    audience: str | None,
    client_id: str,
    issuer: str,
    refresh_token: str,
) -> _RefreshResult:
    """Refresh an expired access token via the OIDC refresh-token grant.

    Mirrors ``refreshAccessToken`` (context.ts:115-182):
    1. Discover the token endpoint at
       ``<issuer>/.well-known/openid-configuration``.
    2. POST a ``grant_type=refresh_token`` form to that endpoint.
    3. Return the new token, expiry, issued-at, and any rotated refresh token
       (some IdPs, e.g. Zitadel, rotate by default).

    No token value is logged or embedded in raised errors.
    """
    if not issuer.startswith("https://"):
        raise RuntimeError("OIDC issuer/token endpoint must use https")
    _ssl_ctx = ssl.create_default_context()
    discovery_url = f"{issuer}/.well-known/openid-configuration"
    try:
        with urllib.request.urlopen(discovery_url, context=_ssl_ctx) as resp:
            discovery = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as err:
        raise RuntimeError(
            f"Failed to fetch OIDC discovery from {discovery_url}: {err.code}"
        ) from None

    token_endpoint = discovery.get("token_endpoint")
    if not token_endpoint:
        raise RuntimeError("missing token_endpoint in OIDC discovery")
    if not token_endpoint.startswith("https://"):
        raise RuntimeError("OIDC issuer/token endpoint must use https")

    form = {
        "grant_type": "refresh_token",
        "client_id": client_id,
        "refresh_token": refresh_token,
    }
    if audience:
        form["audience"] = audience

    body = urllib.parse.urlencode(form).encode("utf-8")
    request = urllib.request.Request(
        token_endpoint,
        data=body,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, context=_ssl_ctx) as resp:
            token_result = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as err:
        # Only the status code is surfaced — never the response body, which may
        # carry a token or error detail.
        raise RuntimeError(f"Token refresh failed with status: {err.code}") from None

    expires_in = token_result.get("expires_in")
    if not isinstance(expires_in, int):
        expires_in = 3600  # default 1 hour

    rotated = token_result.get("refresh_token")
    rotated_refresh_token = (
        rotated if isinstance(rotated, str) and len(rotated) > 0 else None
    )

    return _RefreshResult(
        access_token=token_result["access_token"],
        expires_in=expires_in,
        issued_at=int(time.time()),
        refresh_token=rotated_refresh_token,
    )


def client_auth_header(
    registry: str,
    credentials_path: str = VORPAL_CREDENTIALS_PATH,
) -> str | None:
    """Return the ``Bearer <token>`` header for ``registry``, or ``None``.

    Returns ``None`` when there is no credentials file or no mapping for this
    registry (allowing unauthenticated requests); raises on unrecoverable
    errors. Mirrors ``clientAuthHeader`` (context.ts:195-257) and the Rust/Go
    SDKs: a 5-minute refresh buffer, refresh-token rotation, and a ``0o600``
    rewrite of the refreshed credentials.

    Security: the returned header and all token values are caller-confidential —
    this function logs nothing. A malformed credentials file fails closed with a
    fixed message that never echoes file contents or a token fragment.
    """
    if not os.path.exists(credentials_path):
        return None

    with open(credentials_path, encoding="utf-8") as f:
        raw = f.read()

    try:
        credentials = json.loads(raw)
    except json.JSONDecodeError:
        # Fail closed WITHOUT echoing file contents or the decoder's snippet,
        # which could leak a token fragment.
        raise RuntimeError(
            f"failed to parse credentials file {credentials_path}: invalid JSON"
        ) from None

    def invalid_credentials() -> None:
        raise RuntimeError(
            f"failed to parse credentials file {credentials_path}: invalid credentials"
        ) from None

    if not isinstance(credentials, dict):
        invalid_credentials()

    registry_entries = credentials.get("registry", {})
    if not isinstance(registry_entries, dict):
        invalid_credentials()

    issuer_entries = credentials.get("issuer", {})
    if not isinstance(issuer_entries, dict):
        invalid_credentials()

    missing_registry_mapping = object()
    registry_issuer = registry_entries.get(registry, missing_registry_mapping)
    if registry_issuer is missing_registry_mapping:
        # No registry mapping — allow unauthenticated requests.
        return None
    if not isinstance(registry_issuer, str) or not registry_issuer:
        invalid_credentials()

    issuer_creds = issuer_entries.get(registry_issuer)
    if not isinstance(issuer_creds, dict):
        invalid_credentials()
    if not isinstance(issuer_creds.get("access_token"), str):
        invalid_credentials()
    if type(issuer_creds.get("issued_at")) is not int:
        invalid_credentials()
    if type(issuer_creds.get("expires_in")) is not int:
        invalid_credentials()

    now = int(time.time())
    token_age = now - issuer_creds["issued_at"]
    needs_refresh = token_age + 300 >= issuer_creds["expires_in"]

    if needs_refresh:
        if not issuer_creds.get("refresh_token"):
            raise RuntimeError(
                "Access token expired and no refresh token available. "
                f"Please run: vorpal login --issuer {registry_issuer}"
            )
        if not isinstance(issuer_creds.get("client_id"), str):
            invalid_credentials()

        refreshed = refresh_access_token(
            issuer_creds.get("audience"),
            issuer_creds["client_id"],
            registry_issuer,
            issuer_creds["refresh_token"],
        )

        issuer_creds["access_token"] = refreshed.access_token
        issuer_creds["expires_in"] = refreshed.expires_in
        issuer_creds["issued_at"] = refreshed.issued_at
        if refreshed.refresh_token:
            issuer_creds["refresh_token"] = refreshed.refresh_token

        # Atomic credentials rewrite: write to a tempfile in the same directory
        # (same filesystem → rename is atomic), chmod to 0o600 before content
        # lands, then os.replace() — eliminates the world-readable window and
        # torn-write-on-crash risk.
        creds_dir = os.path.dirname(os.path.abspath(credentials_path))
        tmp_fd, tmp_path = tempfile.mkstemp(dir=creds_dir, suffix=".tmp")
        try:
            os.chmod(tmp_path, 0o600)
            with os.fdopen(tmp_fd, "w", encoding="utf-8") as f:
                json.dump(credentials, f, indent=2)
            os.replace(tmp_path, credentials_path)
        except Exception:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
            raise

    return f"Bearer {issuer_creds['access_token']}"


def _auth_metadata(registry: str) -> list[tuple[str, str]]:
    """Build gRPC call metadata with the bearer header, if credentials exist."""
    bearer = client_auth_header(registry)
    if bearer:
        return [("authorization", bearer)]
    return []


# ---------------------------------------------------------------------------
# Artifact alias parsing — mirrors context.ts:353-510
# ---------------------------------------------------------------------------

DEFAULT_NAMESPACE = "library"
DEFAULT_TAG = "latest"


@dataclass
class ArtifactAlias:
    """Parsed artifact alias: ``[namespace/]name[:tag]``.

    ``namespace`` defaults to ``"library"`` and ``tag`` to ``"latest"``.
    """

    name: str
    namespace: str
    tag: str


def format_artifact_alias(alias: ArtifactAlias) -> str:
    """Format an :class:`ArtifactAlias` to its canonical string.

    Omits the default namespace (``"library"``) and default tag (``"latest"``).
    """
    result = ""
    if alias.namespace != DEFAULT_NAMESPACE:
        result += f"{alias.namespace}/"
    result += alias.name
    if alias.tag != DEFAULT_TAG:
        result += f":{alias.tag}"
    return result


def _is_valid_component(s: str) -> bool:
    if len(s) == 0:
        return False
    for c in s:
        if not (
            ("a" <= c <= "z")
            or ("A" <= c <= "Z")
            or ("0" <= c <= "9")
            or c in "-._+"
        ):
            return False
    return True


def parse_artifact_alias(alias: str) -> ArtifactAlias:
    """Parse an artifact alias string into its components.

    Format ``[namespace/]name[:tag]``; defaults ``namespace="library"``,
    ``tag="latest"``. Valid characters: alphanumeric, ``-``, ``.``, ``_``, ``+``.
    Max length 255. Raises ``ValueError`` on an empty/over-long/malformed alias.
    """
    if len(alias) == 0:
        raise ValueError("alias cannot be empty")
    if len(alias) > 255:
        raise ValueError("alias too long (max 255 characters)")

    # Extract tag on the rightmost ':'.
    last_colon = alias.rfind(":")
    if last_colon != -1:
        tag_part = alias[last_colon + 1 :]
        if tag_part == "":
            raise ValueError("tag cannot be empty")
        tag = tag_part
        base = alias[:last_colon]
    else:
        tag = ""
        base = alias

    # Extract namespace/name.
    slash_idx = base.find("/")
    if slash_idx == -1:
        namespace = ""
        name = base
    else:
        namespace = base[:slash_idx]
        rest = base[slash_idx + 1 :]
        if namespace == "":
            raise ValueError("namespace cannot be empty")
        if "/" in rest:
            raise ValueError("invalid format: too many path separators")
        name = rest

    if name == "":
        raise ValueError("name is required")

    if not _is_valid_component(name):
        raise ValueError(
            "name contains invalid characters (allowed: alphanumeric, "
            "hyphens, dots, underscores, plus signs)"
        )
    if namespace != "" and not _is_valid_component(namespace):
        raise ValueError(
            "namespace contains invalid characters (allowed: alphanumeric, "
            "hyphens, dots, underscores, plus signs)"
        )
    if tag != "" and not _is_valid_component(tag):
        raise ValueError(
            "tag contains invalid characters (allowed: alphanumeric, "
            "hyphens, dots, underscores, plus signs)"
        )

    if tag == "":
        tag = DEFAULT_TAG
    if namespace == "":
        namespace = DEFAULT_NAMESPACE

    return ArtifactAlias(name=name, namespace=namespace, tag=tag)


# ---------------------------------------------------------------------------
# ConfigContext — mirrors context.ts:516-1069
# ---------------------------------------------------------------------------


@dataclass
class _ConfigContextStore:
    artifact: dict[str, artifact_pb2.Artifact] = field(default_factory=dict)
    artifact_input_cache: dict[str, str] = field(default_factory=dict)
    variable: dict[str, str] = field(default_factory=dict)


class ConfigContext:
    """Drives the config lifecycle: validate/add artifacts, fetch from the
    registry, and serve the ``ContextService``. Mirrors Rust ``get_context()``
    and Go ``GetContext()`` in a synchronous model.
    """

    def __init__(
        self,
        artifact: str,
        artifact_context: str,
        artifact_namespace: str,
        artifact_system: artifact_pb2.ArtifactSystem,
        artifact_unlock: bool,
        client_agent: agent_pb2_grpc.AgentServiceStub,
        client_artifact: artifact_pb2_grpc.ArtifactServiceStub,
        port: int,
        registry: str,
        store: _ConfigContextStore,
    ) -> None:
        self._artifact = artifact
        self._artifact_context = artifact_context
        self._artifact_namespace = artifact_namespace
        self._artifact_system = artifact_system
        self._artifact_unlock = artifact_unlock
        self._client_agent = client_agent
        self._client_artifact = client_artifact
        self._port = port
        self._registry = registry
        self._store = store

    @staticmethod
    def create(argv: list[str] | None = None) -> ConfigContext:
        """Parse CLI args, resolve the system, and open the gRPC clients."""
        try:
            args = parse_cli_args(argv)
        except Exception as err:
            raise RuntimeError(
                f"Failed to parse CLI arguments: {err}\n\n"
                "  This usually means the compiled config was invoked\n"
                "  with incorrect or missing arguments. The Vorpal CLI should\n"
                "  supply these automatically during 'vorpal build'.\n\n"
                "  If you are running the config binary manually, the required\n"
                "  arguments are:\n"
                "    start --agent <URL> --artifact <NAME> --artifact-context <PATH>\n"
                "          --artifact-namespace <NS> --artifact-system <SYSTEM>\n"
                "          --port <PORT> --registry <URL>\n"
            ) from None

        try:
            artifact_system = get_system(args.artifact_system)
        except Exception:
            raise RuntimeError(
                f"Unsupported artifact system: '{args.artifact_system}'\n\n"
                "  Supported systems are:\n"
                "    - aarch64-darwin  (Apple Silicon macOS)\n"
                "    - aarch64-linux   (ARM64 Linux)\n"
                "    - x86_64-darwin   (Intel macOS)\n"
                "    - x86_64-linux    (Intel/AMD Linux)\n"
            ) from None

        variables: dict[str, str] = {}
        for v in args.artifact_variable:
            eq_idx = v.find("=")
            if eq_idx != -1:
                variables[v[:eq_idx]] = v[eq_idx + 1 :]

        try:
            client_agent = agent_pb2_grpc.AgentServiceStub(
                _create_channel(args.agent)
            )
        except Exception as err:
            raise RuntimeError(
                f"Failed to connect to agent service at '{args.agent}': {err}\n\n"
                "  Make sure the Vorpal agent is running. You can start it with:\n"
                "    vorpal system services start\n\n"
                "  If using a custom agent address, verify the --agent URL is "
                "correct.\n"
            ) from None

        try:
            client_artifact = artifact_pb2_grpc.ArtifactServiceStub(
                _create_channel(args.registry)
            )
        except Exception as err:
            raise RuntimeError(
                f"Failed to connect to registry service at '{args.registry}': {err}\n\n"
                "  Make sure the Vorpal registry is running. You can start it with:\n"
                "    vorpal system services start\n\n"
                "  If using a custom registry address, verify the --registry URL "
                "is correct.\n"
            ) from None

        return ConfigContext(
            args.artifact,
            args.artifact_context,
            args.artifact_namespace,
            artifact_system,
            args.artifact_unlock,
            client_agent,
            client_artifact,
            args.port,
            args.registry,
            _ConfigContextStore(variable=variables),
        )

    def add_artifact(self, artifact: artifact_pb2.Artifact) -> str:
        """Validate an artifact, compute its digest, and send it to the agent.

        The SHA-256 digest is computed from the cross-SDK JSON serialization.
        Mirrors ``addArtifact`` (context.ts:666-787).
        """
        if artifact.name == "":
            raise ValueError("name cannot be empty")
        if len(artifact.steps) == 0:
            raise ValueError("steps cannot be empty")
        if len(artifact.systems) == 0:
            raise ValueError("systems cannot be empty")

        if artifact.target not in artifact.systems:
            supported = ", ".join(str(s) for s in artifact.systems)
            raise ValueError(
                f"artifact '{artifact.name}' does not support system "
                f"'{artifact.target}' (supported: {supported})"
            )

        # CRITICAL PATH for cross-SDK parity.
        artifact_digest = hashlib.sha256(artifact_to_json_bytes(artifact)).hexdigest()

        if artifact_digest in self._store.artifact:
            return artifact_digest

        cached_output = self._store.artifact_input_cache.get(artifact_digest)
        if cached_output and cached_output in self._store.artifact:
            return cached_output

        input_digest = artifact_digest

        request = agent_pb2.PrepareArtifactRequest(
            artifact_unlock=self._artifact_unlock,
            artifact_context=self._artifact_context,
            artifact_namespace=self._artifact_namespace,
            registry=self._registry,
            artifact=artifact,
        )

        response_artifact: artifact_pb2.Artifact | None = None
        response_digest: str | None = None

        try:
            stream = self._client_agent.PrepareArtifact(
                request, metadata=_auth_metadata(self._registry)
            )
            for response in stream:
                if response.artifact_output:
                    print(f"{artifact.name} |> {response.artifact_output}")
                if response.HasField("artifact"):
                    response_artifact = response.artifact
                if response.artifact_digest:
                    response_digest = response.artifact_digest
        except grpc.RpcError as err:
            raise self._map_agent_error(err, artifact.name) from None

        if response_artifact is None:
            raise RuntimeError("artifact not returned from agent service")
        if response_digest is None:
            raise RuntimeError("artifact digest not returned from agent service")

        self._store.artifact[response_digest] = response_artifact
        self._store.artifact_input_cache[input_digest] = response_digest

        return response_digest

    @staticmethod
    def _map_agent_error(err: grpc.RpcError, name: str) -> RuntimeError:
        code = err.code()
        if code == grpc.StatusCode.NOT_FOUND:
            return RuntimeError(
                f"Artifact '{name}' not found in agent service.\n\n"
                "  The agent does not have this artifact registered.\n"
                "  This can happen if the agent was restarted or the artifact\n"
                "  has not been built yet.\n"
            )
        if code == grpc.StatusCode.UNAVAILABLE:
            return RuntimeError(
                "Agent service is unavailable (connection refused or dropped).\n\n"
                "  Could not reach the agent at the configured address.\n\n"
                "  To fix this:\n"
                "    1. Make sure the Vorpal agent is running:\n"
                "         vorpal system services start\n"
                "    2. Check that the agent address is correct in your config.\n"
            )
        if code == grpc.StatusCode.DEADLINE_EXCEEDED:
            return RuntimeError(
                f"Agent service request timed out for artifact '{name}'.\n\n"
                "  The agent took too long to respond. This may indicate:\n"
                "    - The agent is overloaded or under heavy build load\n"
                "    - Network connectivity issues between client and agent\n\n"
                "  Try again, or check agent logs for more details.\n"
            )
        return RuntimeError(
            f"gRPC error from agent service (code={code}): {err.details()}\n\n"
            "  An unexpected error occurred while communicating with the agent.\n"
            "  Check the agent logs for more details.\n"
        )

    def fetch_artifact(self, digest: str) -> str:
        """Fetch an artifact by digest from the registry, recursing on deps."""
        return self._fetch_artifact_in_namespace(digest, self._artifact_namespace)

    def _fetch_artifact_in_namespace(self, digest: str, namespace: str) -> str:
        if digest in self._store.artifact:
            return digest

        request = artifact_pb2.ArtifactRequest(digest=digest, namespace=namespace)

        try:
            artifact = self._client_artifact.GetArtifact(
                request, metadata=_auth_metadata(self._registry)
            )
        except grpc.RpcError as err:
            raise self._map_registry_fetch_error(err, digest) from None

        self._store.artifact[digest] = artifact

        for step in artifact.steps:
            for dep in step.artifacts:
                self._fetch_artifact_in_namespace(dep, namespace)

        return digest

    def _map_registry_fetch_error(
        self, err: grpc.RpcError, digest: str
    ) -> RuntimeError:
        code = err.code()
        if code == grpc.StatusCode.NOT_FOUND:
            return RuntimeError(
                f"Artifact not found in registry (digest: {digest}).\n\n"
                "  The registry does not have an artifact with this digest.\n"
                "  This can happen if the artifact was never pushed or has been "
                "pruned.\n"
            )
        if code == grpc.StatusCode.UNAVAILABLE:
            return RuntimeError(
                "Registry service is unavailable.\n\n"
                f"  Could not reach the registry at '{self._registry}'.\n\n"
                "  To fix this:\n"
                "    1. Make sure the Vorpal registry is running:\n"
                "         vorpal system services start\n"
                "    2. Check that the registry address is correct.\n"
            )
        return RuntimeError(
            f"Registry service error (code={code}): {err.details()}\n\n"
            f"  An unexpected error occurred while fetching artifact '{digest}'.\n"
            "  Check the registry logs for more details.\n"
        )

    def fetch_artifact_alias(self, alias: str) -> str:
        """Fetch an artifact by alias from the registry (Go FetchArtifactAlias)."""
        parsed = parse_artifact_alias(alias)

        request = artifact_pb2.GetArtifactAliasRequest(
            system=self._artifact_system,
            name=parsed.name,
            namespace=parsed.namespace,
            tag=parsed.tag,
        )

        try:
            response = self._client_artifact.GetArtifactAlias(
                request, metadata=_auth_metadata(self._registry)
            )
        except grpc.RpcError as err:
            raise self._map_registry_alias_error(err, alias, parsed) from None

        artifact_digest = response.digest
        if not artifact_digest:
            raise RuntimeError(f"Registry returned empty digest for alias: {alias}")

        if artifact_digest in self._store.artifact:
            return artifact_digest

        self._fetch_artifact_in_namespace(artifact_digest, parsed.namespace)

        return artifact_digest

    def _map_registry_alias_error(
        self, err: grpc.RpcError, alias: str, parsed: ArtifactAlias
    ) -> RuntimeError:
        code = err.code()
        if code == grpc.StatusCode.NOT_FOUND:
            return RuntimeError(
                f"Artifact alias '{alias}' not found in registry.\n\n"
                f"  No artifact matches namespace='{parsed.namespace}', "
                f"name='{parsed.name}', tag='{parsed.tag}'.\n\n"
                "  Make sure the artifact has been built and published,\n"
                "  and that the alias is spelled correctly.\n"
            )
        if code == grpc.StatusCode.UNAVAILABLE:
            return RuntimeError(
                "Registry service is unavailable.\n\n"
                f"  Could not reach the registry at '{self._registry}'.\n\n"
                "  To fix this:\n"
                "    1. Make sure the Vorpal registry is running:\n"
                "         vorpal system services start\n"
                "    2. Check that the registry address is correct.\n"
            )
        return RuntimeError(
            f"Registry error fetching alias '{alias}' (code={code}): "
            f"{err.details()}\n\n"
            "  Check the registry logs for more details.\n"
        )

    def get_artifact_store(self) -> dict[str, artifact_pb2.Artifact]:
        """Return a shallow copy of the artifact store (digest -> Artifact)."""
        return dict(self._store.artifact)

    def get_artifact(self, digest: str) -> artifact_pb2.Artifact | None:
        """Look up a previously registered artifact by its digest."""
        return self._store.artifact.get(digest)

    def get_artifact_context_path(self) -> str:
        """Return the filesystem path to the artifact context directory."""
        return self._artifact_context

    def get_artifact_name(self) -> str:
        """Return the name of the top-level artifact being built."""
        return self._artifact

    def get_artifact_namespace(self) -> str:
        """Return the namespace used for artifact registration and lookup."""
        return self._artifact_namespace

    def get_system(self) -> artifact_pb2.ArtifactSystem:
        """Return the target :class:`ArtifactSystem` for this build."""
        return self._artifact_system

    def get_variable(self, name: str) -> str | None:
        """Look up a build variable passed via ``--artifact-variable KEY=VALUE``."""
        return self._store.variable.get(name)

    def run(self) -> None:
        """Start the ``ContextService`` gRPC server until SIGINT/SIGTERM.

        Prints ``context service: [::]:PORT`` to stdout for CLI detection,
        mirroring ``ConfigContext.run`` (context.ts:989-1068).

        Must be called on the main thread — Python only permits
        ``signal.signal`` calls from the main thread.
        """
        server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
        context_pb2_grpc.add_ContextServiceServicer_to_server(
            _ContextServicer(self._store), server
        )

        addr = f"[::]:{self._port}"
        bound = server.add_insecure_port(addr)
        if bound == 0:
            raise RuntimeError(
                f"Failed to bind context service to {addr}\n\n"
                "  The config's gRPC context server could not start.\n"
                "  This usually means the port is already in use by another "
                "process.\n\n"
                "  To fix this:\n"
                "    1. Check if another Vorpal config process is still running\n"
                "    2. Try running 'vorpal build' again (a new port will be "
                "selected)\n"
            )

        server.start()

        print(f"context service: {addr}", flush=True)

        def _shutdown(signum: int, frame: Any) -> None:
            # grace=0: stop accepting calls and release the port promptly so an
            # immediate re-bind succeeds (mirrors the TS tryShutdown/forceShutdown).
            server.stop(0)

        signal.signal(signal.SIGINT, _shutdown)
        signal.signal(signal.SIGTERM, _shutdown)

        server.wait_for_termination()


class _ContextServicer(context_pb2_grpc.ContextServiceServicer):
    """Serves artifacts registered during this config run. Mirrors the
    ``ContextService`` handlers in ``context.ts:994-1029``.
    """

    def __init__(self, store: _ConfigContextStore) -> None:
        self._store = store

    def GetArtifact(
        self, request: artifact_pb2.ArtifactRequest, context: grpc.ServicerContext
    ) -> artifact_pb2.Artifact:
        if not request.digest:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, "'digest' is required")

        artifact = self._store.artifact.get(request.digest)
        if artifact is None:
            context.abort(grpc.StatusCode.NOT_FOUND, "artifact not found")

        return artifact

    def GetArtifacts(
        self, request: artifact_pb2.ArtifactsRequest, context: grpc.ServicerContext
    ) -> artifact_pb2.ArtifactsResponse:
        digests = sorted(self._store.artifact.keys())
        return artifact_pb2.ArtifactsResponse(digests=digests)
