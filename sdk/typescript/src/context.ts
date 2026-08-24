import { createHash, randomBytes } from "node:crypto";
import {
  closeSync,
  existsSync,
  fsyncSync,
  openSync,
  readFileSync,
  renameSync,
  unlinkSync,
  writeSync,
} from "node:fs";
import { basename, dirname, join } from "node:path";
import * as grpc from "@grpc/grpc-js";
import type {
  Artifact as ArtifactMsg,
  ArtifactSource as ArtifactSourceMsg,
  ArtifactStep as ArtifactStepMsg,
  ArtifactStepSecret,
  ArtifactRequest,
  ArtifactsRequest,
  ArtifactsResponse,
} from "./api/artifact/artifact.js";
import {
  ArtifactSystem,
  ArtifactServiceClient,
} from "./api/artifact/artifact.js";
import {
  AgentServiceClient,
} from "./api/agent/agent.js";
import type {
  PrepareArtifactRequest,
  PrepareArtifactResponse,
} from "./api/agent/agent.js";
import {
  ContextServiceService,
} from "./api/context/context.js";
import { parseCliArgs } from "./cli.js";
import { getSystem } from "./system.js";

// ---------------------------------------------------------------------------
// TLS credential helper — matches Rust get_client_tls_config()
// ---------------------------------------------------------------------------

const VORPAL_ROOT_DIR = "/var/lib/vorpal";
const VORPAL_CA_PATH = join(VORPAL_ROOT_DIR, "key", "ca.pem");

function getClientCredentials(uri: string): grpc.ChannelCredentials {
  if (uri.startsWith("http://") || uri.startsWith("unix://")) {
    return grpc.credentials.createInsecure();
  }

  if (existsSync(VORPAL_CA_PATH)) {
    const caPem = readFileSync(VORPAL_CA_PATH);
    return grpc.credentials.createSsl(caPem);
  }

  // Use system roots (createSsl with no args uses Node's default CA store)
  return grpc.credentials.createSsl();
}

/**
 * Converts a URI to a gRPC-js compatible target string.
 *
 * `@grpc/grpc-js` does not understand `https://` or `http://` schemes —
 * it expects `host:port`, `dns:///host:port`, or `unix:///path`.
 * The Rust SDK's `tonic` handles `https://` natively, so the CLI passes
 * registry/agent URLs in that format. We convert them here.
 */
function toGrpcTarget(uri: string): string {
  if (uri.startsWith("unix://")) {
    return uri;
  }
  if (uri.startsWith("https://")) {
    const host = uri.slice("https://".length).replace(/\/+$/, "");
    return host.includes(":") ? host : `${host}:443`;
  }
  if (uri.startsWith("http://")) {
    const host = uri.slice("http://".length).replace(/\/+$/, "");
    return host.includes(":") ? host : `${host}:80`;
  }
  return uri;
}

// ---------------------------------------------------------------------------
// Credential types and auth header — matches Rust client_auth_header()
// and Go ClientAuthHeader()
// ---------------------------------------------------------------------------

const VORPAL_CREDENTIALS_PATH = join(VORPAL_ROOT_DIR, "key", "credentials.json");

/** Matches VorpalCredentialsContent (Go) / IssuerCredentials (Rust). */
interface IssuerCredentials {
  access_token: string;
  audience?: string;
  client_id: string;
  expires_in: number;
  issued_at: number;
  refresh_token: string;
  scopes: string[];
}

/** Matches VorpalCredentials (Go/Rust). */
interface VorpalCredentials {
  issuer: Record<string, IssuerCredentials>;
  registry: Record<string, string>;
}

/** Minimal OIDC discovery document — we only need `token_endpoint`. */
interface OIDCDiscovery {
  token_endpoint: string;
}

/**
 * A completed refresh exchange. `expiresIn` is `undefined` when the IdP's
 * response omitted the field — distinct from an explicit `0`, which
 * {@link commitRefreshedCredentials} refuses to persist (C-10). `refreshToken`
 * is set only when the IdP rotated the refresh token (Zitadel default);
 * `undefined` means the caller keeps the existing one.
 */
interface RefreshedToken {
  accessToken: string;
  expiresIn: number | undefined;
  issuedAt: number;
  refreshToken?: string;
}

/**
 * Why a refresh exchange failed, discriminated by whether the refresh token
 * had already left this process. `sent === false` means the fault was local
 * (bad URL, unreachable discovery endpoint, malformed document) and the
 * stored token is untouched — safe to retry. `sent === true` means the
 * token-endpoint request was issued, so the IdP may have consumed the token
 * whatever came back — it must never be sent a second time. Mirrors Rust's
 * `RefreshFailure` enum and Go's `refreshFailureError`.
 */
class RefreshFailure extends Error {
  readonly sent: boolean;

  constructor(message: string, sent: boolean, options?: ErrorOptions) {
    super(message, options);
    this.name = "RefreshFailure";
    this.sent = sent;
  }
}

function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function notSent(cause: unknown): RefreshFailure {
  return new RefreshFailure(errMessage(cause), false, { cause });
}

function sent(cause: unknown): RefreshFailure {
  return new RefreshFailure(errMessage(cause), true, { cause });
}

/** Every discovery and token-endpoint request is bounded by this timeout
 * (C-9). C-1's process-wide lock is held across the whole exchange, so a
 * hung IdP would otherwise stall every authenticated call in the process. */
const REFRESH_HTTP_TIMEOUT_MS = 30_000;

/**
 * Parses an OIDC URL into its `scheme://host:port` origin, refusing any
 * destination a refresh token must not be sent to. Plaintext HTTP is
 * refused except on loopback, where there is no network to eavesdrop and
 * local IdP fixtures live. Mirrors Rust's `credential_egress_origin`
 * (context.rs:688-712) and Go's `credentialEgressOrigin`.
 */
/** @internal Exported for unit tests only; not part of the public SDK surface. */
export function credentialEgressOrigin(raw: string): string {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(`invalid OIDC URL: ${raw}`);
  }

  // WHATWG URL keeps the bracket syntax in `hostname` for an IPv6 literal
  // (`[::1]`, not `::1`), unlike Go's url.Hostname(). Strip it before
  // comparing so the loopback exemption actually matches "::1" rather than
  // silently refusing every IPv6-loopback issuer.
  const host = url.hostname.replace(/^\[|\]$/g, "");
  if (!host) {
    throw new Error(`OIDC URL has no host: ${raw}`);
  }

  const isLoopback = host === "localhost" || host === "127.0.0.1" || host === "::1";

  if (url.protocol === "https:") {
    // ok
  } else if (url.protocol === "http:" && isLoopback) {
    // ok
  } else {
    throw new Error(
      `refusing to send a refresh token over ${url.protocol.replace(":", "")} to ${host}: the OIDC issuer must be https`,
    );
  }

  const port = url.port || (url.protocol === "https:" ? "443" : "80");

  return `${url.protocol}//${host}:${port}`;
}

/**
 * Refreshes an expired access token using the OIDC refresh-token grant.
 *
 * Mirrors `refresh_access_token` in Rust and `refreshAccessToken` in Go:
 * 1. Discover the token endpoint via `<issuer>/.well-known/openid-configuration`
 * 2. POST a `grant_type=refresh_token` form to the token endpoint
 * 3. Return the new access token, expiry, issued-at timestamp, and any
 *    rotated refresh token (some IdPs, e.g. Zitadel, rotate by default)
 *
 * Every error is classified via {@link notSent}/{@link sent} at the point it
 * arises: everything up to and including the discovery round trip happens
 * before the token is on the wire (C-4). No redirect is followed on either
 * request and both are bounded by {@link REFRESH_HTTP_TIMEOUT_MS} (C-9): a
 * redirect followed here previously let a compromised token endpoint replay
 * the credential-bearing POST to a host of its choosing (AB-10, REPRODUCED
 * against Node's default fetch during the VPL-189 threat model).
 *
 * @internal Exported for unit tests only; not part of the public SDK surface.
 */
export async function refreshAccessToken(
  audience: string | undefined,
  clientId: string,
  issuer: string,
  refreshToken: string,
): Promise<RefreshedToken> {
  let issuerOrigin: string;
  try {
    issuerOrigin = credentialEgressOrigin(issuer);
  } catch (err) {
    throw notSent(err);
  }

  // Discover token endpoint. Strip a trailing slash first: an issuer of
  // "https://idp.example/" would otherwise double the slash and 404 rather
  // than reach the discovery document.
  const discoveryUrl = `${issuer.replace(/\/+$/, "")}/.well-known/openid-configuration`;
  let discoveryResp: Response;
  try {
    discoveryResp = await fetch(discoveryUrl, {
      redirect: "error",
      signal: AbortSignal.timeout(REFRESH_HTTP_TIMEOUT_MS),
    });
  } catch (err) {
    throw notSent(new Error(`failed to fetch OIDC discovery from ${discoveryUrl}: ${errMessage(err)}`));
  }
  if (!discoveryResp.ok) {
    throw notSent(new Error(`OIDC discovery failed with status: ${discoveryResp.status}`));
  }

  const discovery = (await discoveryResp.json()) as OIDCDiscovery;
  if (!discovery.token_endpoint) {
    throw notSent(new Error("missing token_endpoint in OIDC discovery"));
  }

  let tokenEndpointOrigin: string;
  try {
    tokenEndpointOrigin = credentialEgressOrigin(discovery.token_endpoint);
  } catch (err) {
    throw notSent(err);
  }

  // Contract, not incidental: the discovery document's token_endpoint must
  // share the issuer's scheme://host:port — closes the egress-redirection
  // hazard in which a tampered or compromised discovery document steers the
  // credential-bearing POST to a host of its own choosing.
  if (tokenEndpointOrigin !== issuerOrigin) {
    throw notSent(
      new Error(
        `OIDC token_endpoint origin ${tokenEndpointOrigin} does not match issuer origin ${issuerOrigin}`,
      ),
    );
  }

  // Build refresh token request (application/x-www-form-urlencoded)
  const params = new URLSearchParams();
  params.set("grant_type", "refresh_token");
  params.set("client_id", clientId);
  params.set("refresh_token", refreshToken);
  if (audience) {
    params.set("audience", audience);
  }

  // From here on the token is on the wire: a transport error, a timeout and
  // a rejection are indistinguishable from the IdP having consumed it.
  let tokenResp: Response;
  try {
    tokenResp = await fetch(discovery.token_endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: params.toString(),
      redirect: "error",
      signal: AbortSignal.timeout(REFRESH_HTTP_TIMEOUT_MS),
    });
  } catch (err) {
    throw sent(new Error(`failed to refresh token: ${errMessage(err)}`));
  }

  if (!tokenResp.ok) {
    throw sent(new Error(`token refresh failed with status: ${tokenResp.status}`));
  }

  let tokenResult: { access_token: string; expires_in?: number; refresh_token?: string };
  try {
    // `tokenResp.json()` only guarantees valid JSON, not the shape below —
    // an unchecked cast here would let a malformed or hostile token
    // response (e.g. a bare `null`, an array, or a body missing
    // `access_token`) either throw an unclassified TypeError past this
    // function's Sent/NotSent boundary (so the caller never marks the
    // token spent and replays it — the token is already on the wire by
    // this point) or silently persist an unusable access token. Validate
    // the shape here, inside the same Sent boundary as the parse failure
    // above, rather than trusting the cast.
    const parsed: unknown = await tokenResp.json();
    if (typeof parsed !== "object" || parsed === null) {
      throw new Error("token response is not a JSON object");
    }
    const candidate = parsed as Record<string, unknown>;
    if (typeof candidate.access_token !== "string" || candidate.access_token.length === 0) {
      throw new Error("token response is missing a non-empty access_token");
    }
    if (candidate.expires_in !== undefined && typeof candidate.expires_in !== "number") {
      throw new Error("token response's expires_in must be a number when present");
    }
    if (candidate.refresh_token !== undefined && typeof candidate.refresh_token !== "string") {
      throw new Error("token response's refresh_token must be a string when present");
    }
    tokenResult = candidate as typeof tokenResult;
  } catch (err) {
    throw sent(new Error(`failed to parse token response: ${errMessage(err)}`));
  }

  // IdPs that rotate refresh tokens (e.g., Zitadel) return a new one here;
  // some send an empty string rather than omitting the field, which must
  // not overwrite the stored refresh token with an empty value.
  const rotatedRefreshToken =
    typeof tokenResult.refresh_token === "string" && tokenResult.refresh_token.length > 0
      ? tokenResult.refresh_token
      : undefined;

  return {
    accessToken: tokenResult.access_token,
    expiresIn: tokenResult.expires_in,
    issuedAt: Math.floor(Date.now() / 1000),
    refreshToken: rotatedRefreshToken,
  };
}

async function liveRefresher(
  audience: string | undefined,
  clientId: string,
  issuer: string,
  refreshToken: string,
): Promise<RefreshedToken> {
  return refreshAccessToken(audience, clientId, issuer, refreshToken);
}

/**
 * Decides whether the stored access token must be refreshed before use.
 * Pure in its three inputs so the policy is checkable with no file, no lock
 * and no network.
 *
 * A future-dated `issuedAt` (clock skew, or a hostile file — the value is
 * file-sourced and unvalidated beyond {@link validateIssuerCredentials})
 * means the token's real age is unknown, and unknown age fails toward
 * refreshing: clamping it to zero would make a stale token look freshly
 * issued and suppress a refresh that is genuinely due — the fail-open
 * direction VPL-183 reverses in Rust (context.rs:1185-1194).
 *
 * Rust, Go and TypeScript deliberately agree on this skew-safe direction. A
 * later "restore cross-SDK parity" pass must not revert it back to
 * suppress-on-skew — the divergence from the pre-VPL-183 behavior is
 * intentional, not drift.
 */
/** @internal Exported for unit tests only; not part of the public SDK surface. */
export function needsRefresh(issuedAt: number, expiresIn: number, now: number): boolean {
  if (issuedAt > now) {
    return true;
  }

  const tokenAge = now - issuedAt;
  const window = Math.min(300, Math.floor(expiresIn / 2));

  return tokenAge + window >= expiresIn;
}

function digestHex(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

/**
 * The error every arm that spends the stored refresh token throws: what
 * went wrong locally, that the grant is over, and the one command that
 * restores it. `cause` is inlined into the message because every production
 * caller formats only the outermost message, and preserved via
 * `ErrorOptions.cause` for programmatic inspection.
 */
function spentGrantError(issuer: string, summary: string, cause: unknown): Error {
  return new Error(`${summary} (${errMessage(cause)}). Please run: vorpal login --issuer ${issuer}`, { cause });
}

/**
 * Validates a single issuer credential record at first contact (C-11).
 * TypeScript's `interface` types are erased at runtime and provide no
 * validation whatever, so a corrupt or hostile field (e.g. a non-numeric
 * `issued_at`) would otherwise silently propagate as `NaN` into
 * {@link needsRefresh} and suppress a due refresh (AB-7b).
 */
function validateIssuerCredentials(issuerName: string, value: unknown): asserts value is IssuerCredentials {
  if (typeof value !== "object" || value === null) {
    throw new Error(`credentials for issuer ${issuerName} is not an object`);
  }

  const v = value as Record<string, unknown>;

  if (typeof v.access_token !== "string") {
    throw new Error(`access_token for issuer ${issuerName} must be a string`);
  }
  if (typeof v.refresh_token !== "string") {
    throw new Error(`refresh_token for issuer ${issuerName} must be a string`);
  }
  if (typeof v.client_id !== "string") {
    throw new Error(`client_id for issuer ${issuerName} must be a string`);
  }
  if (typeof v.issued_at !== "number" || !Number.isFinite(v.issued_at)) {
    throw new Error(`issued_at for issuer ${issuerName} must be a finite number`);
  }
  if (v.issued_at < 0) {
    throw new Error(`issued_at for issuer ${issuerName} must not be negative`);
  }
  if (typeof v.expires_in !== "number" || !Number.isFinite(v.expires_in)) {
    throw new Error(`expires_in for issuer ${issuerName} must be a finite number`);
  }
  if (v.expires_in < 0) {
    throw new Error(`expires_in for issuer ${issuerName} must not be negative`);
  }
  if (v.audience !== undefined && typeof v.audience !== "string") {
    throw new Error(`audience for issuer ${issuerName} must be a string when present`);
  }
}

/**
 * Parses and validates a credentials.json document (C-11). Rejects the file
 * outright rather than letting an invalid record reach {@link needsRefresh}
 * or a refresh exchange.
 */
function parseCredentials(raw: string): VorpalCredentials {
  const parsed: unknown = JSON.parse(raw);
  if (typeof parsed !== "object" || parsed === null) {
    throw new Error("credentials file does not contain a JSON object");
  }

  const obj = parsed as Record<string, unknown>;
  const issuerField = obj.issuer;
  const registryField = obj.registry;

  if (typeof issuerField !== "object" || issuerField === null) {
    throw new Error("credentials file missing 'issuer' object");
  }
  if (typeof registryField !== "object" || registryField === null) {
    throw new Error("credentials file missing 'registry' object");
  }

  for (const [issuerName, value] of Object.entries(issuerField as Record<string, unknown>)) {
    validateIssuerCredentials(issuerName, value);
  }

  return { issuer: issuerField, registry: registryField } as VorpalCredentials;
}

/** Bounds the retry against a temp-file name collision (C-5). TypeScript has
 * no `mkstemp` equivalent, so name generation, `wx` exclusivity and this
 * bounded retry are all authored rather than inherited from Rust. */
const TEMP_FILE_MAX_ATTEMPTS = 8;

function tempFileCandidateName(path: string): string {
  return `${basename(path)}.${process.pid}.${randomBytes(8).toString("hex")}.tmp`;
}

/**
 * Writes `data` to `path` atomically: a temp file in the same directory
 * (required for `renameSync` to be atomic — it is only atomic within a
 * filesystem) is created exclusively (`wx`, i.e. `O_CREAT | O_EXCL`) at
 * mode 0600, written, `fsync`'d, and renamed onto `path` (C-5). `renameSync`
 * replaces the destination inode with the source inode, so the
 * destination's mode after the rename is the temp file's mode, not any
 * pre-existing destination mode (closes AB-6). The temp file is unlinked on
 * every failure path (closes AB-13).
 */
function writeCredentialsSecure(path: string, data: string): void {
  const dir = dirname(path);
  let lastErr: unknown;

  for (let attempt = 0; attempt < TEMP_FILE_MAX_ATTEMPTS; attempt++) {
    const tmpPath = join(dir, tempFileCandidateName(path));

    let fd: number;
    try {
      fd = openSync(tmpPath, "wx", 0o600);
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === "EEXIST") {
        lastErr = err;
        continue;
      }
      throw err;
    }

    let closed = false;
    let committed = false;
    try {
      // writeSync is not guaranteed to write the whole buffer in one call;
      // a discarded short-write return value would let a partial write be
      // fsync'd and renamed onto path as if it were complete (SEC-1).
      const expectedBytes = Buffer.byteLength(data, "utf-8");
      const writtenBytes = writeSync(fd, data, null, "utf-8");
      if (writtenBytes !== expectedBytes) {
        throw new Error(
          `short write to temp credentials file: wrote ${writtenBytes} of ${expectedBytes} bytes`,
        );
      }
      // fsync, not just close: a rename ordered ahead of the data reaching
      // disk can leave a zero-length credentials file after a crash.
      fsyncSync(fd);
      closeSync(fd);
      closed = true;
      renameSync(tmpPath, path);
      committed = true;
    } finally {
      if (!closed) {
        try {
          closeSync(fd);
        } catch {
          /* already closed or never opened */
        }
      }
      if (!committed) {
        try {
          unlinkSync(tmpPath);
        } catch {
          /* best effort */
        }
      }
    }

    return;
  }

  throw new Error(
    `every one of ${TEMP_FILE_MAX_ATTEMPTS} candidate temp-file names was already taken writing ${path}` +
      (lastErr ? `: ${errMessage(lastErr)}` : ""),
  );
}

/**
 * Applies a completed exchange to `credentials` and writes the result to
 * `path`. Throws without committing anything when the IdP's response is
 * unusable (C-10): an absent `expiresIn` defaults to 3600, but an explicit
 * value `<= 0` can never satisfy {@link needsRefresh}'s window, which would
 * make every later call rotate again (AB-11) — refused instead.
 */
function commitRefreshedCredentials(
  credentials: VorpalCredentials,
  issuer: string,
  path: string,
  refreshed: RefreshedToken,
): void {
  let expires: number;
  if (refreshed.expiresIn === undefined) {
    expires = 3600;
  } else if (refreshed.expiresIn <= 0) {
    throw new Error(
      `OAuth refresh for issuer ${issuer} returned a token with a zero lifetime. Please run: vorpal login --issuer ${issuer}`,
    );
  } else {
    expires = refreshed.expiresIn;
  }

  const issuerCreds = credentials.issuer[issuer];
  if (!issuerCreds) {
    throw new Error(`no credentials for issuer: ${issuer}`);
  }

  issuerCreds.access_token = refreshed.accessToken;
  issuerCreds.expires_in = expires;
  issuerCreds.issued_at = refreshed.issuedAt;
  // Persist rotated refresh token when the IdP returned one; leave the
  // existing value untouched when omitted (some IdPs do not rotate and
  // reuse the original refresh token).
  if (refreshed.refreshToken) {
    issuerCreds.refresh_token = refreshed.refreshToken;
  }

  writeCredentialsSecure(path, JSON.stringify(credentials, null, 2));
}

/**
 * Serializes the whole critical section — read, refresh decision, exchange,
 * write — within this process (C-1), and owns `spent`, the memo of
 * refresh-token digests this process has already put on the wire without
 * durably committing a replacement (C-3).
 *
 * `spent` lives on the same object as the lock chain rather than as a
 * second, separately declared module-level global: every read or write of
 * it happens inside {@link withCredentialsRefreshLock}'s callback, so this
 * bundling makes that a structural fact of the state's shape
 * rather than an unstated obligation a future edit could violate by adding
 * a new top-level `let`. Mirrors Rust's `Mutex<RefreshState>`
 * (`context.rs:1140-1166`), which the language enforces at compile time;
 * TypeScript has no equivalent guarantee, so the single-object shape is the
 * closest available discipline.
 *
 * Node/Bun are single-threaded, but every `await` is an interleaving point:
 * `await`-ing the chain tail before the existence check and releasing after
 * the write is what makes this a real mutex rather than a no-op — a
 * synchronous critical section would not need it, but this one spans
 * `await fetch(...)` twice.
 *
 * Scope is this process only: it does not serialize against a separately
 * spawned config process, a running `vorpal start agent`, or the Rust/Go
 * SDKs writing the same `credentials.json` — an accepted residual risk
 * shared with Rust (context.rs:1160-1166) and Go.
 */
const credentialsRefreshState: { chain: Promise<void>; spent: Set<string> } = {
  chain: Promise.resolve(),
  spent: new Set<string>(),
};

function withCredentialsRefreshLock<T>(fn: () => Promise<T>): Promise<T> {
  const result = credentialsRefreshState.chain.then(fn);
  // The chain link always resolves regardless of fn's outcome, so a
  // rejection from one caller never poisons the next waiter's turn.
  credentialsRefreshState.chain = result.then(
    () => undefined,
    () => undefined,
  );
  return result;
}

/**
 * Retrieves the authorization header for a given registry.
 *
 * Returns `Bearer <token>` if valid credentials exist, `null` if no
 * credentials file or no mapping for this registry (allowing
 * unauthenticated requests), or throws on unrecoverable errors.
 *
 * Matches Rust `client_auth_header()` / `client_auth_header_at()` and Go
 * `ClientAuthHeader()` / `clientAuthHeaderAt()`. `credentialsPath` already
 * made this function's core testable without touching the real
 * `/var/lib/vorpal/key/credentials.json` (C-12 required no change here);
 * `refresher` and `now` extend that same parameter shape rather than
 * becoming module-level mutable exports, so a test can drive the refresh
 * exchange and the clock without real network I/O or the wall clock.
 *
 * `now` is called only after {@link withCredentialsRefreshLock}'s chain tail
 * is awaited — never before (C-2). A value sampled before acquiring the
 * lock can predate a still-in-flight winner's later commit; a waiter that
 * then compares its stale, pre-lock reading against the winner's freshly
 * committed `issued_at` sees a future-dated token and refreshes again, once
 * per waiter (AB-2).
 *
 * @internal Exported for unit tests only; not part of the public SDK surface.
 */
export async function clientAuthHeader(
  registry: string,
  credentialsPath: string = VORPAL_CREDENTIALS_PATH,
  refresher: typeof liveRefresher = liveRefresher,
  now: () => number = () => Math.floor(Date.now() / 1000),
): Promise<string | null> {
  return withCredentialsRefreshLock(async () => {
    // Read here, strictly after the lock — see this function's doc comment.
    const nowUnix = now();

    if (!existsSync(credentialsPath)) {
      return null;
    }

    const credentialsData = readFileSync(credentialsPath, "utf-8");
    const credentials = parseCredentials(credentialsData);

    // A plain-object index lookup falls through to the prototype chain for
    // a registry named e.g. "constructor" or "toString", returning an
    // inherited function instead of undefined. hasOwnProperty rules that
    // out so an unmapped registry always takes the "no mapping" branch.
    const registryIssuer = Object.prototype.hasOwnProperty.call(credentials.registry, registry)
      ? credentials.registry[registry]
      : undefined;
    if (!registryIssuer) {
      // No registry mapping — allow unauthenticated requests
      return null;
    }

    let issuerCreds = credentials.issuer[registryIssuer];
    if (!issuerCreds) {
      throw new Error(`no credentials for issuer: ${registryIssuer}`);
    }

    if (needsRefresh(issuerCreds.issued_at, issuerCreds.expires_in, nowUnix)) {
      if (!issuerCreds.refresh_token) {
        throw new Error(
          `Access token expired and no refresh token available. Please run: vorpal login --issuer ${registryIssuer}`,
        );
      }

      // A prior caller may already have put this exact stored token value
      // on the wire. The file it left behind is byte-identical either way,
      // so the memo is the only thing that can tell the two apart.
      const refreshTokenDigest = digestHex(issuerCreds.refresh_token);

      if (credentialsRefreshState.spent.has(refreshTokenDigest)) {
        throw new Error(
          `OAuth refresh-token exchange already failed for the stored token. Please run: vorpal login --issuer ${registryIssuer}`,
        );
      }

      let refreshed: RefreshedToken;
      try {
        refreshed = await refresher(
          issuerCreds.audience,
          issuerCreds.client_id,
          registryIssuer,
          issuerCreds.refresh_token,
        );
      } catch (err) {
        if (err instanceof RefreshFailure && err.sent) {
          credentialsRefreshState.spent.add(refreshTokenDigest);
          throw spentGrantError(
            registryIssuer,
            `the OAuth refresh-token exchange for issuer ${registryIssuer} failed after the token had been sent, so the stored refresh token is no longer usable`,
            err,
          );
        }

        // NotSent, or an unclassified error: the token never left the
        // process, the stored token is untouched, and a later caller may
        // use it.
        throw new Error(`failed to refresh token: ${errMessage(err)}`);
      }

      // No `await` is introduced between the exchange above and the commit
      // below: the sequence stays synchronous so the window in which a
      // killed process loses the rotated token to disk (accepted residual
      // risk) does not widen.
      try {
        commitRefreshedCredentials(credentials, registryIssuer, credentialsPath, refreshed);
      } catch (err) {
        // The exchange happened and nothing was committed, so the file
        // still names a token the IdP may have already rotated away. This
        // is the same replay hazard as an outright failure and it is spent
        // for the same reason.
        credentialsRefreshState.spent.add(refreshTokenDigest);

        throw spentGrantError(
          registryIssuer,
          `refreshed credentials for issuer ${registryIssuer} could not be saved, so the stored refresh token is no longer usable`,
          err,
        );
      }

      issuerCreds = credentials.issuer[registryIssuer];
    }

    return `Bearer ${issuerCreds.access_token}`;
  });
}

// ---------------------------------------------------------------------------
// Custom JSON serialization for cross-SDK parity
// ---------------------------------------------------------------------------

/**
 * Serializes an Artifact to JSON bytes matching Rust's serde_json::to_vec
 * output for prost-generated structs.
 *
 * Key differences from the generated toJSON:
 * - Field names are snake_case (matching proto field names)
 * - Field order follows proto field number order
 * - ALL fields are always included (even zero-values, empty arrays)
 * - Enums serialize as integers (not strings)
 * - Optional None serializes as null
 *
 * This matches what serde_json produces for prost structs with
 * #[derive(Serialize)] -- all fields present, in declaration order,
 * with no skip_serializing_if attributes.
 */
export function serializeArtifactStepSecret(secret: ArtifactStepSecret): object {
  return {
    name: secret.name,
    value: secret.value,
  };
}

/**
 * Serializes an {@link ArtifactSource} to a plain object matching Rust's serde output.
 * Field order and inclusion rules match the cross-SDK parity requirements.
 */
export function serializeArtifactSource(source: ArtifactSourceMsg): object {
  return {
    digest: source.digest ?? null,
    excludes: source.excludes,
    includes: source.includes,
    name: source.name,
    path: source.path,
  };
}

/**
 * Serializes an {@link ArtifactStep} to a plain object matching Rust's serde output.
 * Field order and inclusion rules match the cross-SDK parity requirements.
 */
export function serializeArtifactStep(step: ArtifactStepMsg): object {
  return {
    entrypoint: step.entrypoint ?? null,
    script: step.script ?? null,
    secrets: step.secrets.map(serializeArtifactStepSecret),
    arguments: step.arguments,
    artifacts: step.artifacts,
    environments: step.environments,
  };
}

/**
 * Serializes an {@link Artifact} to a plain object matching Rust's serde output.
 * Field order and inclusion rules match the cross-SDK parity requirements.
 */
export function serializeArtifact(artifact: ArtifactMsg): object {
  return {
    target: artifact.target,
    sources: artifact.sources.map(serializeArtifactSource),
    steps: artifact.steps.map(serializeArtifactStep),
    systems: artifact.systems,
    aliases: artifact.aliases,
    name: artifact.name,
  };
}

/**
 * Serializes an Artifact to a JSON string matching Rust serde_json::to_vec.
 * Returns the UTF-8 bytes of the JSON string.
 */
export function artifactToJsonBytes(artifact: ArtifactMsg): Buffer {
  const obj = serializeArtifact(artifact);
  const json = JSON.stringify(obj);
  return Buffer.from(json, "utf-8");
}

/**
 * Computes the SHA-256 digest of an artifact using the cross-SDK-compatible
 * JSON serialization. The returned hex string is identical to what the
 * Rust and Go SDKs produce for the same artifact definition.
 */
export function computeArtifactDigest(artifact: ArtifactMsg): string {
  const jsonBytes = artifactToJsonBytes(artifact);
  return createHash("sha256").update(jsonBytes).digest("hex");
}

// ---------------------------------------------------------------------------
// Artifact alias parsing
// ---------------------------------------------------------------------------

const DEFAULT_NAMESPACE = "library";
const DEFAULT_TAG = "latest";

/**
 * Parsed representation of an artifact alias string.
 *
 * Alias format: `[namespace/]name[:tag]`
 *
 * Defaults:
 * - `namespace` defaults to `"library"`
 * - `tag` defaults to `"latest"`
 *
 * @example
 * ```typescript
 * const alias = parseArtifactAlias("my-tool:v1.0");
 * // { name: "my-tool", namespace: "library", tag: "v1.0" }
 * ```
 */
export interface ArtifactAlias {
  /** Artifact name (required) */
  name: string;
  /** Namespace, defaults to `"library"` */
  namespace: string;
  /** Version tag, defaults to `"latest"` */
  tag: string;
}

/**
 * Formats an ArtifactAlias back into its canonical string representation.
 * Omits default namespace ("library") and default tag ("latest").
 */
export function formatArtifactAlias(alias: ArtifactAlias): string {
  const hasNamespace = alias.namespace !== DEFAULT_NAMESPACE;
  const hasTag = alias.tag !== DEFAULT_TAG;

  let result = "";
  if (hasNamespace) {
    result += `${alias.namespace}/`;
  }
  result += alias.name;
  if (hasTag) {
    result += `:${alias.tag}`;
  }
  return result;
}

function isValidComponent(s: string): boolean {
  if (s.length === 0) return false;
  for (const c of s) {
    if (
      !(
        (c >= "a" && c <= "z") ||
        (c >= "A" && c <= "Z") ||
        (c >= "0" && c <= "9") ||
        c === "-" ||
        c === "." ||
        c === "_" ||
        c === "+"
      )
    ) {
      return false;
    }
  }
  return true;
}

/**
 * Parses an artifact alias string into its component parts.
 *
 * Format: `[namespace/]name[:tag]`
 *
 * - If no namespace is provided, defaults to `"library"`.
 * - If no tag is provided, defaults to `"latest"`.
 * - Valid characters: alphanumeric, hyphens, dots, underscores, plus signs.
 * - Maximum length: 255 characters.
 *
 * @param alias - The alias string to parse
 * @returns Parsed {@link ArtifactAlias} with defaults applied
 * @throws If the alias is empty, too long, or contains invalid characters
 */
export function parseArtifactAlias(alias: string): ArtifactAlias {
  if (alias.length === 0) {
    throw new Error("alias cannot be empty");
  }

  if (alias.length > 255) {
    throw new Error("alias too long (max 255 characters)");
  }

  // Step 1: Extract tag (split on rightmost ':')
  let base: string;
  let tag: string;
  const lastColon = alias.lastIndexOf(":");
  if (lastColon !== -1) {
    const tagPart = alias.substring(lastColon + 1);
    if (tagPart === "") {
      throw new Error("tag cannot be empty");
    }
    tag = tagPart;
    base = alias.substring(0, lastColon);
  } else {
    tag = "";
    base = alias;
  }

  // Step 2: Extract namespace/name
  let namespace: string;
  let name: string;
  const slashIdx = base.indexOf("/");
  if (slashIdx === -1) {
    namespace = "";
    name = base;
  } else {
    namespace = base.substring(0, slashIdx);
    const rest = base.substring(slashIdx + 1);
    if (namespace === "") {
      throw new Error("namespace cannot be empty");
    }
    if (rest.includes("/")) {
      throw new Error("invalid format: too many path separators");
    }
    name = rest;
  }

  if (name === "") {
    throw new Error("name is required");
  }

  // Step 3: Validate component characters
  if (!isValidComponent(name)) {
    throw new Error(
      "name contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  if (namespace !== "" && !isValidComponent(namespace)) {
    throw new Error(
      "namespace contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  if (tag !== "" && !isValidComponent(tag)) {
    throw new Error(
      "tag contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  // Step 4: Apply defaults
  if (tag === "") {
    tag = DEFAULT_TAG;
  }

  if (namespace === "") {
    namespace = DEFAULT_NAMESPACE;
  }

  return { name, namespace, tag };
}

// ---------------------------------------------------------------------------
// ConfigContextStore
// ---------------------------------------------------------------------------

interface ConfigContextStore {
  artifact: Map<string, ArtifactMsg>;
  artifactInputCache: Map<string, string>;
  variable: Map<string, string>;
}

// ---------------------------------------------------------------------------
// ConfigContext
// ---------------------------------------------------------------------------

export class ConfigContext {
  private _artifact: string;
  private _artifactContext: string;
  private _artifactNamespace: string;
  private _artifactSystem: ArtifactSystem;
  private _artifactUnlock: boolean;
  private _clientAgent: AgentServiceClient;
  private _clientArtifact: ArtifactServiceClient;
  private _port: number;
  private _registry: string;
  private _store: ConfigContextStore;

  private constructor(
    artifact: string,
    artifactContext: string,
    artifactNamespace: string,
    artifactSystem: ArtifactSystem,
    artifactUnlock: boolean,
    clientAgent: AgentServiceClient,
    clientArtifact: ArtifactServiceClient,
    port: number,
    registry: string,
    store: ConfigContextStore,
  ) {
    this._artifact = artifact;
    this._artifactContext = artifactContext;
    this._artifactNamespace = artifactNamespace;
    this._artifactSystem = artifactSystem;
    this._artifactUnlock = artifactUnlock;
    this._clientAgent = clientAgent;
    this._clientArtifact = clientArtifact;
    this._port = port;
    this._registry = registry;
    this._store = store;
  }

  /**
   * Creates a ConfigContext by parsing CLI arguments and connecting to
   * gRPC services. Matches Rust get_context() and Go GetContext().
   */
  static create(argv?: string[]): ConfigContext {
    let args: ReturnType<typeof parseCliArgs>;
    try {
      args = parseCliArgs(argv);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(
        `Failed to parse CLI arguments: ${msg}\n\n` +
        `  This usually means the compiled TypeScript config was invoked\n` +
        `  with incorrect or missing arguments. The Vorpal CLI should\n` +
        `  supply these automatically during 'vorpal build'.\n\n` +
        `  If you are running the config binary manually, the required\n` +
        `  arguments are:\n` +
        `    start --agent <URL> --artifact <NAME> --artifact-context <PATH>\n` +
        `          --artifact-namespace <NS> --artifact-system <SYSTEM>\n` +
        `          --port <PORT> --registry <URL>\n`,
      );
    }

    let artifactSystem: ArtifactSystem;
    try {
      artifactSystem = getSystem(args.artifactSystem);
    } catch (_err) {
      throw new Error(
        `Unsupported artifact system: '${args.artifactSystem}'\n\n` +
        `  Supported systems are:\n` +
        `    - aarch64-darwin  (Apple Silicon macOS)\n` +
        `    - aarch64-linux   (ARM64 Linux)\n` +
        `    - x86_64-darwin   (Intel macOS)\n` +
        `    - x86_64-linux    (Intel/AMD Linux)\n`,
      );
    }

    // Parse variables
    const variables = new Map<string, string>();
    for (const v of args.artifactVariable) {
      const eqIdx = v.indexOf("=");
      if (eqIdx !== -1) {
        const name = v.substring(0, eqIdx);
        const value = v.substring(eqIdx + 1);
        variables.set(name, value);
      }
    }

    // Create gRPC clients (TLS based on URI scheme, matching Rust SDK)
    let clientAgent: AgentServiceClient;
    let clientArtifact: ArtifactServiceClient;
    try {
      const agentTarget = toGrpcTarget(args.agent);
      const agentCredentials = getClientCredentials(args.agent);
      clientAgent = new AgentServiceClient(agentTarget, agentCredentials);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(
        `Failed to connect to agent service at '${args.agent}': ${msg}\n\n` +
        `  Make sure the Vorpal agent is running. You can start it with:\n` +
        `    vorpal system services start\n\n` +
        `  If using a custom agent address, verify the --agent URL is correct.\n`,
      );
    }
    try {
      const registryTarget = toGrpcTarget(args.registry);
      const registryCredentials = getClientCredentials(args.registry);
      clientArtifact = new ArtifactServiceClient(registryTarget, registryCredentials);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(
        `Failed to connect to registry service at '${args.registry}': ${msg}\n\n` +
        `  Make sure the Vorpal registry is running. You can start it with:\n` +
        `    vorpal system services start\n\n` +
        `  If using a custom registry address, verify the --registry URL is correct.\n`,
      );
    }

    return new ConfigContext(
      args.artifact,
      args.artifactContext,
      args.artifactNamespace,
      artifactSystem,
      args.artifactUnlock,
      clientAgent,
      clientArtifact,
      args.port,
      args.registry,
      {
        artifact: new Map(),
        artifactInputCache: new Map(),
        variable: variables,
      },
    );
  }

  /**
   * Adds an artifact to the context, computing its digest and sending it
   * to the agent service for preparation.
   *
   * The SHA-256 digest is computed from the JSON serialization of the
   * artifact, using the custom serializer that matches Rust's
   * serde_json::to_vec output.
   */
  async addArtifact(artifact: ArtifactMsg): Promise<string> {
    if (artifact.name === "") {
      throw new Error("name cannot be empty");
    }

    if (artifact.steps.length === 0) {
      throw new Error("steps cannot be empty");
    }

    if (artifact.systems.length === 0) {
      throw new Error("systems cannot be empty");
    }

    // Validate target is in systems list
    if (!artifact.systems.includes(artifact.target)) {
      throw new Error(
        `artifact '${artifact.name}' does not support system '${artifact.target}' (supported: ${artifact.systems.join(", ")})`,
      );
    }

    // Serialize and compute digest -- CRITICAL PATH for cross-SDK parity
    const artifactJson = artifactToJsonBytes(artifact);
    const artifactDigest = createHash("sha256")
      .update(artifactJson)
      .digest("hex");

    if (this._store.artifact.has(artifactDigest)) {
      return artifactDigest;
    }

    const cachedOutputDigest = this._store.artifactInputCache.get(artifactDigest);
    if (cachedOutputDigest && this._store.artifact.has(cachedOutputDigest)) {
      return cachedOutputDigest;
    }

    const inputDigest = artifactDigest;

    // Send to agent for preparation
    const request: PrepareArtifactRequest = {
      artifact_unlock: this._artifactUnlock,
      artifact_context: this._artifactContext,
      artifact_namespace: this._artifactNamespace,
      registry: this._registry,
      artifact: artifact,
    };

    // Add authorization metadata if credentials exist (matches Go/Rust SDKs)
    const metadata = new grpc.Metadata();
    const bearerToken = await clientAuthHeader(this._registry);
    if (bearerToken) {
      metadata.set("authorization", bearerToken);
    }

    const stream = this._clientAgent.prepareArtifact(request, metadata);

    let responseArtifact: ArtifactMsg | undefined;
    let responseArtifactDigest: string | undefined;

    await new Promise<void>((resolve, reject) => {
      stream.on("data", (response: PrepareArtifactResponse) => {
        if (response.artifact_output) {
          console.log(`${artifact.name} |> ${response.artifact_output}`);
        }

        if (response.artifact) {
          responseArtifact = response.artifact;
        }

        if (response.artifact_digest) {
          responseArtifactDigest = response.artifact_digest;
        }
      });

      stream.on("end", () => resolve());
      stream.on("error", (err: grpc.ServiceError) => {
        if (err.code === grpc.status.NOT_FOUND) {
          reject(new Error(
            `Artifact '${artifact.name}' not found in agent service.\n\n` +
            `  The agent does not have this artifact registered.\n` +
            `  This can happen if the agent was restarted or the artifact\n` +
            `  has not been built yet.\n`,
          ));
        } else if (err.code === grpc.status.UNAVAILABLE) {
          reject(new Error(
            `Agent service is unavailable (connection refused or dropped).\n\n` +
            `  Could not reach the agent at the configured address.\n\n` +
            `  To fix this:\n` +
            `    1. Make sure the Vorpal agent is running:\n` +
            `         vorpal system services start\n` +
            `    2. Check that the agent address is correct in your config.\n`,
          ));
        } else if (err.code === grpc.status.DEADLINE_EXCEEDED) {
          reject(new Error(
            `Agent service request timed out for artifact '${artifact.name}'.\n\n` +
            `  The agent took too long to respond. This may indicate:\n` +
            `    - The agent is overloaded or under heavy build load\n` +
            `    - Network connectivity issues between client and agent\n\n` +
            `  Try again, or check agent logs for more details.\n`,
          ));
        } else {
          reject(new Error(
            `gRPC error from agent service (code=${err.code}): ${err.message}\n\n` +
            `  An unexpected error occurred while communicating with the agent.\n` +
            `  Check the agent logs for more details.\n`,
          ));
        }
      });
    });

    if (!responseArtifact) {
      throw new Error("artifact not returned from agent service");
    }

    if (!responseArtifactDigest) {
      throw new Error("artifact digest not returned from agent service");
    }

    this._store.artifact.set(responseArtifactDigest, responseArtifact);
    this._store.artifactInputCache.set(inputDigest, responseArtifactDigest);

    return responseArtifactDigest;
  }

  /**
   * Fetches an artifact by digest from the artifact service (registry).
   * Recursively fetches all step dependencies.
   */
  async fetchArtifact(digest: string): Promise<string> {
    return this.fetchArtifactInNamespace(digest, this._artifactNamespace);
  }

  /**
   * Fetches an artifact by digest in a specific namespace.
   * Recursively fetches all step dependencies using the same namespace.
   * This mirrors the Rust SDK's `fetch_artifact_in_namespace`.
   */
  private async fetchArtifactInNamespace(digest: string, namespace: string): Promise<string> {
    if (this._store.artifact.has(digest)) {
      return digest;
    }

    const request: ArtifactRequest = {
      digest: digest,
      namespace: namespace,
    };

    // Add authorization metadata if credentials exist (matches Go/Rust SDKs)
    const metadata = new grpc.Metadata();
    const bearerToken = await clientAuthHeader(this._registry);
    if (bearerToken) {
      metadata.set("authorization", bearerToken);
    }

    const artifact = await new Promise<ArtifactMsg>((resolve, reject) => {
      this._clientArtifact.getArtifact(request, metadata, (err, response) => {
        if (err) {
          const svcErr = err as grpc.ServiceError;
          if (svcErr.code === grpc.status.NOT_FOUND) {
            reject(new Error(
              `Artifact not found in registry (digest: ${digest}).\n\n` +
              `  The registry does not have an artifact with this digest.\n` +
              `  This can happen if the artifact was never pushed or has been pruned.\n`,
            ));
          } else if (svcErr.code === grpc.status.UNAVAILABLE) {
            reject(new Error(
              `Registry service is unavailable.\n\n` +
              `  Could not reach the registry at '${this._registry}'.\n\n` +
              `  To fix this:\n` +
              `    1. Make sure the Vorpal registry is running:\n` +
              `         vorpal system services start\n` +
              `    2. Check that the registry address is correct.\n`,
            ));
          } else {
            reject(new Error(
              `Registry service error (code=${svcErr.code}): ${err.message}\n\n` +
              `  An unexpected error occurred while fetching artifact '${digest}'.\n` +
              `  Check the registry logs for more details.\n`,
            ));
          }
        } else {
          resolve(response);
        }
      });
    });

    this._store.artifact.set(digest, artifact);

    for (const step of artifact.steps) {
      for (const dep of step.artifacts) {
        await this.fetchArtifactInNamespace(dep, namespace);
      }
    }

    return digest;
  }

  /**
   * Fetches an artifact by alias from the artifact service (registry).
   * Uses the Go SDK approach: FetchArtifactAlias.
   */
  async fetchArtifactAlias(alias: string): Promise<string> {
    const parsed = parseArtifactAlias(alias);

    const request = {
      system: this._artifactSystem,
      name: parsed.name,
      namespace: parsed.namespace,
      tag: parsed.tag,
    };

    // Add authorization metadata if credentials exist (matches Go/Rust SDKs)
    const metadata = new grpc.Metadata();
    const bearerToken = await clientAuthHeader(this._registry);
    if (bearerToken) {
      metadata.set("authorization", bearerToken);
    }

    const response = await new Promise<{ digest: string }>((resolve, reject) => {
      this._clientArtifact.getArtifactAlias(request, metadata, (err, resp) => {
        if (err) {
          const svcErr = err as grpc.ServiceError;
          if (svcErr.code === grpc.status.NOT_FOUND) {
            reject(new Error(
              `Artifact alias '${alias}' not found in registry.\n\n` +
              `  No artifact matches namespace='${parsed.namespace}', ` +
              `name='${parsed.name}', tag='${parsed.tag}'.\n\n` +
              `  Make sure the artifact has been built and published,\n` +
              `  and that the alias is spelled correctly.\n`,
            ));
          } else if (svcErr.code === grpc.status.UNAVAILABLE) {
            reject(new Error(
              `Registry service is unavailable.\n\n` +
              `  Could not reach the registry at '${this._registry}'.\n\n` +
              `  To fix this:\n` +
              `    1. Make sure the Vorpal registry is running:\n` +
              `         vorpal system services start\n` +
              `    2. Check that the registry address is correct.\n`,
            ));
          } else {
            reject(new Error(
              `Registry error fetching alias '${alias}' (code=${svcErr.code}): ${err.message}\n\n` +
              `  Check the registry logs for more details.\n`,
            ));
          }
        } else {
          resolve(resp);
        }
      });
    });

    const artifactDigest = response.digest;

    if (!artifactDigest) {
      throw new Error(`Registry returned empty digest for alias: ${alias}`);
    }

    if (this._store.artifact.has(artifactDigest)) {
      return artifactDigest;
    }

    await this.fetchArtifactInNamespace(artifactDigest, parsed.namespace);

    return artifactDigest;
  }

  /**
   * Returns a shallow copy of the artifact store (digest -> Artifact).
   * Useful for inspecting all artifacts registered during this config run.
   */
  getArtifactStore(): Map<string, ArtifactMsg> {
    return new Map(this._store.artifact);
  }

  /**
   * Looks up a previously registered artifact by its digest.
   *
   * @param digest - The hex-encoded SHA-256 digest
   * @returns The artifact, or `undefined` if not found
   */
  getArtifact(digest: string): ArtifactMsg | undefined {
    return this._store.artifact.get(digest);
  }

  /** Returns the filesystem path to the artifact context directory. */
  getArtifactContextPath(): string {
    return this._artifactContext;
  }

  /** Returns the name of the top-level artifact being built. */
  getArtifactName(): string {
    return this._artifact;
  }

  /** Returns the namespace used for artifact registration and lookup. */
  getArtifactNamespace(): string {
    return this._artifactNamespace;
  }

  /**
   * Returns the target {@link ArtifactSystem} for this build
   * (e.g., `ArtifactSystem.AARCH64_DARWIN`).
   */
  getSystem(): ArtifactSystem {
    return this._artifactSystem;
  }

  /**
   * Looks up a build variable by name. Variables are passed via
   * `--artifact-variable KEY=VALUE` on the CLI.
   *
   * @param name - Variable name
   * @returns The variable value, or `undefined` if not set
   */
  getVariable(name: string): string | undefined {
    return this._store.variable.get(name);
  }

  /**
   * Starts the ContextService gRPC server.
   * Matches Rust ConfigContext::run() and Go ConfigContext.Run().
   *
   * Prints "context service: [::]:PORT" to stdout for CLI detection.
   */
  async run(): Promise<void> {
    const server = new grpc.Server();

    const store = this._store;

    server.addService(ContextServiceService, {
      getArtifact: (
        call: grpc.ServerUnaryCall<ArtifactRequest, ArtifactMsg>,
        callback: grpc.sendUnaryData<ArtifactMsg>,
      ) => {
        const request = call.request;

        if (!request.digest || request.digest === "") {
          callback({
            code: grpc.status.INVALID_ARGUMENT,
            message: "'digest' is required",
          });
          return;
        }

        const artifact = store.artifact.get(request.digest);

        if (!artifact) {
          callback({
            code: grpc.status.NOT_FOUND,
            message: "artifact not found",
          });
          return;
        }

        callback(null, artifact);
      },

      getArtifacts: (
        _call: grpc.ServerUnaryCall<ArtifactsRequest, ArtifactsResponse>,
        callback: grpc.sendUnaryData<ArtifactsResponse>,
      ) => {
        const digests = Array.from(store.artifact.keys()).sort();
        callback(null, { digests });
      },
    });

    const addr = `[::]:${this._port}`;

    await new Promise<void>((resolve, reject) => {
      server.bindAsync(addr, grpc.ServerCredentials.createInsecure(), (err) => {
        if (err) {
          reject(new Error(
            `Failed to bind context service to ${addr}: ${err.message}\n\n` +
            `  The TypeScript config's gRPC context server could not start.\n` +
            `  This usually means the port is already in use by another process.\n\n` +
            `  To fix this:\n` +
            `    1. Check if another Vorpal config process is still running\n` +
            `    2. Try running 'vorpal build' again (a new port will be selected)\n`,
          ));
          return;
        }
        resolve();
      });
    });

    console.log(`context service: ${addr}`);

    // Keep the server running until SIGINT/SIGTERM
    await new Promise<void>((resolve) => {
      const shutdown = () => {
        server.tryShutdown((err) => {
          if (err) {
            console.error(`Warning: context service shutdown error: ${err.message}`);
            console.error(`  Forcing shutdown. This is usually harmless.`);
            server.forceShutdown();
          }
          resolve();
        });
      };

      process.on("SIGINT", shutdown);
      process.on("SIGTERM", shutdown);
    });
  }
}
