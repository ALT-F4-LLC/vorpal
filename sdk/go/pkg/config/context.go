package config

import (
	"context"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/agent"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	apiContext "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/context"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/metadata"
)

type ArtifactAlias struct {
	Name      string
	Namespace string
	Tag       string
}

type ConfigContextStore struct {
	artifact           map[string]*artifact.Artifact
	artifactInputCache map[string]string
	variable           map[string]string
}

type ConfigContext struct {
	artifact          string
	artifactContext   string
	artifactNamespace string
	artifactSystem    artifact.ArtifactSystem
	artifactSystemStr string
	artifactUnlock    bool
	clientAgent       agent.AgentServiceClient
	clientArtifact    artifact.ArtifactServiceClient
	port              int
	registry          string
	store             ConfigContextStore
}

// VorpalCredentialsContent represents OIDC credentials for an issuer
type VorpalCredentialsContent struct {
	AccessToken  string   `json:"access_token"`
	Audience     *string  `json:"audience,omitempty"`
	ClientId     string   `json:"client_id"`
	ExpiresIn    int64    `json:"expires_in"`
	IssuedAt     int64    `json:"issued_at"`
	RefreshToken string   `json:"refresh_token"`
	Scopes       []string `json:"scopes"`
}

// VorpalCredentials represents the credentials file structure
type VorpalCredentials struct {
	Issuer   map[string]VorpalCredentialsContent `json:"issuer"`
	Registry map[string]string                   `json:"registry"`
}

// OIDCDiscovery represents the OIDC discovery document
type OIDCDiscovery struct {
	TokenEndpoint string `json:"token_endpoint"`
}

type ConfigServer struct {
	apiContext.UnimplementedContextServiceServer

	store ConfigContextStore
}

func NewConfigServer(store ConfigContextStore) *ConfigServer {
	return &ConfigServer{
		store: store,
	}
}

func (s *ConfigServer) GetArtifact(ctx context.Context, request *artifact.ArtifactRequest) (*artifact.Artifact, error) {
	if request.Digest == "" {
		return nil, fmt.Errorf("'digest' is required")
	}

	response := s.store.artifact[request.Digest]
	if response == nil {
		return nil, fmt.Errorf("artifact not found")
	}

	return response, nil
}

func (s *ConfigServer) GetArtifacts(ctx context.Context, request *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error) {
	digests := make([]string, 0)

	for digest := range s.store.artifact {
		digests = append(digests, digest)
	}

	sort.Strings(digests)

	response := &artifact.ArtifactsResponse{
		Digests: digests,
	}

	return response, nil
}

// refreshHTTPTimeout bounds every discovery and token-endpoint request. This
// matters because C-1's process-wide lock is held across the whole exchange:
// without a timeout, a hung IdP stalls every authenticated call in the
// process, not just its own.
const refreshHTTPTimeout = 30 * time.Second

// refreshFailureKind classifies why a refresh exchange failed, by whether the
// refresh token had already left this process. NotSent means the fault was
// local (bad URL, unreachable discovery endpoint, malformed document) and the
// stored token is untouched — safe to retry. Sent means the token-endpoint
// request was issued, so the IdP may have consumed the token whatever came
// back — it must never be sent a second time.
type refreshFailureKind int

const (
	refreshFailureNotSent refreshFailureKind = iota
	refreshFailureSent
)

// refreshFailureError wraps an error with its refreshFailureKind. Rust's
// equivalent is the RefreshFailure enum in sdk/rust/src/context.rs.
type refreshFailureError struct {
	kind refreshFailureKind
	err  error
}

func (e *refreshFailureError) Error() string { return e.err.Error() }
func (e *refreshFailureError) Unwrap() error { return e.err }

func notSentErr(err error) error { return &refreshFailureError{kind: refreshFailureNotSent, err: err} }
func sentErr(err error) error    { return &refreshFailureError{kind: refreshFailureSent, err: err} }

// credentialEgressOrigin parses an OIDC URL into its scheme://host:port
// origin, refusing any destination a refresh token must not be sent to.
// Plaintext HTTP is refused except on loopback, where there is no network to
// eavesdrop and local IdP fixtures live. Mirrors Rust's
// credential_egress_origin (context.rs:688-712).
func credentialEgressOrigin(raw string) (string, error) {
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("invalid OIDC URL: %s: %w", raw, err)
	}

	host := u.Hostname()
	if host == "" {
		return "", fmt.Errorf("OIDC URL has no host: %s", raw)
	}

	isLoopback := host == "localhost" || host == "127.0.0.1" || host == "::1"

	switch {
	case u.Scheme == "https":
	case u.Scheme == "http" && isLoopback:
	default:
		return "", fmt.Errorf("refusing to send a refresh token over %s to %s: the OIDC issuer must be https", u.Scheme, host)
	}

	port := u.Port()
	if port == "" {
		switch u.Scheme {
		case "https":
			port = "443"
		case "http":
			port = "80"
		default:
			return "", fmt.Errorf("OIDC URL has no port: %s", raw)
		}
	}

	return fmt.Sprintf("%s://%s:%s", u.Scheme, host, port), nil
}

// refreshHTTPClient returns an *http.Client with the timeout and redirect
// policy every refresh-exchange request must carry (C-9): no redirect is
// followed (a 307 surfaces as a non-200 to the existing status check rather
// than being replayed to a host of the redirector's choosing), and the whole
// request is bounded so a hung IdP cannot stall the process-wide lock.
func refreshHTTPClient(timeout time.Duration) *http.Client {
	return &http.Client{
		Timeout: timeout,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
}

// refreshAccessToken refreshes an expired access token using the refresh
// token. Returns the new access token, its expires_in (nil when the IdP's
// response omitted the field, distinct from an explicit zero — C-10), its
// issued-at timestamp, and an optional rotated refresh token (empty string
// when the IdP did not rotate). Every error is classified as a
// *refreshFailureError: NotSent up through and including the discovery round
// trip, Sent from the token-endpoint request onward.
func refreshAccessToken(audience *string, clientId, issuer, refreshToken string, timeout time.Duration) (string, *int64, int64, string, error) {
	client := refreshHTTPClient(timeout)

	issuerOrigin, err := credentialEgressOrigin(issuer)
	if err != nil {
		return "", nil, 0, "", notSentErr(err)
	}

	// Discover token endpoint
	discoveryURL := fmt.Sprintf("%s/.well-known/openid-configuration", issuer)
	resp, err := client.Get(discoveryURL)
	if err != nil {
		return "", nil, 0, "", notSentErr(fmt.Errorf("failed to fetch OIDC discovery: %w", err))
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", nil, 0, "", notSentErr(fmt.Errorf("OIDC discovery failed with status: %d", resp.StatusCode))
	}

	var discovery OIDCDiscovery
	if err := json.NewDecoder(resp.Body).Decode(&discovery); err != nil {
		return "", nil, 0, "", notSentErr(fmt.Errorf("failed to parse OIDC discovery: %w", err))
	}

	tokenEndpointOrigin, err := credentialEgressOrigin(discovery.TokenEndpoint)
	if err != nil {
		return "", nil, 0, "", notSentErr(err)
	}

	// Contract, not incidental: the discovery document's token_endpoint must
	// share the issuer's scheme://host:port, closing the egress-redirection
	// hazard in which a tampered or compromised discovery document steers the
	// credential-bearing POST to a host of its own choosing.
	if tokenEndpointOrigin != issuerOrigin {
		return "", nil, 0, "", notSentErr(fmt.Errorf(
			"OIDC token_endpoint origin %s does not match issuer origin %s",
			tokenEndpointOrigin, issuerOrigin,
		))
	}

	// Build refresh token request
	data := url.Values{}
	data.Set("grant_type", "refresh_token")
	data.Set("client_id", clientId)
	data.Set("refresh_token", refreshToken)
	if audience != nil {
		data.Set("audience", *audience)
	}

	// From here on the token is on the wire: a transport error, a timeout and
	// a rejection are indistinguishable from the IdP having consumed it.
	tokenResp, err := client.PostForm(discovery.TokenEndpoint, data)
	if err != nil {
		return "", nil, 0, "", sentErr(fmt.Errorf("failed to refresh token: %w", err))
	}
	defer tokenResp.Body.Close()

	if tokenResp.StatusCode != http.StatusOK {
		return "", nil, 0, "", sentErr(fmt.Errorf("token refresh failed with status: %d", tokenResp.StatusCode))
	}

	var tokenResult struct {
		AccessToken  string `json:"access_token"`
		ExpiresIn    *int64 `json:"expires_in"`
		RefreshToken string `json:"refresh_token"`
	}
	if err := json.NewDecoder(tokenResp.Body).Decode(&tokenResult); err != nil {
		return "", nil, 0, "", sentErr(fmt.Errorf("failed to parse token response: %w", err))
	}

	issuedAt := time.Now().Unix()

	return tokenResult.AccessToken, tokenResult.ExpiresIn, issuedAt, tokenResult.RefreshToken, nil
}

// needsRefresh decides whether the stored access token must be refreshed
// before use. Pure in its three inputs so the policy is checkable with no
// file, no lock and no network. A future-dated issuedAt (clock skew, or a
// hostile file — the value is file-sourced and unvalidated) means the
// token's real age is unknown, and unknown age fails toward refreshing:
// clamping it to zero would make a stale token look freshly issued and
// suppress a refresh that is genuinely due — the fail-open direction VPL-183
// reverses in Rust (context.rs:1185-1194), agreed here on purpose.
//
// Rust, Go and TypeScript deliberately agree on this skew-safe direction. A
// later "restore cross-SDK parity" pass must not revert it back to
// suppress-on-skew — the divergence from the pre-VPL-183 behavior is
// intentional, not drift.
func needsRefresh(issuedAt, expiresIn, now int64) bool {
	if issuedAt > now {
		return true
	}

	tokenAge := now - issuedAt
	window := expiresIn / 2
	if window > 300 {
		window = 300
	}

	return tokenAge+window >= expiresIn
}

// digestHex returns the hex-encoded SHA-256 digest of s, used to key the
// spent-refresh-token memo (C-3) without ever storing the plaintext value.
func digestHex(s string) string {
	sum := sha256.Sum256([]byte(s))
	return hex.EncodeToString(sum[:])
}

// spentGrantError is the error every arm that spends the stored refresh
// token returns: what went wrong locally, that the grant is over, and the
// one command that restores it. cause is inlined into the message because
// every production caller wraps this error and formats only the outermost
// message, so a cause left in the chain alone would be invisible to the
// user; the chain is preserved via %w for programmatic inspection.
func spentGrantError(issuer, summary string, cause error) error {
	return fmt.Errorf("%s (%w). Please run: vorpal login --issuer %s", summary, cause, issuer)
}

// tempFileMaxAttempts bounds writeCredentialsSecure's retry against a
// collision on the temp-file name. os.CreateTemp already draws a random
// suffix and retries internally on O_EXCL collision, so this is a ceiling on
// top of that, not the primary collision handling.
const tempFileMaxAttempts = 8

// writeCredentialsSecure writes data to path atomically: a temp file in the
// same directory (required for os.Rename to be atomic — it is only atomic
// within a filesystem) is created exclusively at mode 0600, written,
// fsync'd, and renamed onto path. os.Rename replaces the destination inode
// with the source inode, so the destination's mode after the rename is the
// temp file's mode, not any pre-existing destination mode (C-5, closing
// AB-6). The temp file is unlinked on every failure path (closing AB-13).
func writeCredentialsSecure(path string, data []byte) error {
	dir := filepath.Dir(path)
	base := filepath.Base(path)

	var lastErr error

	for attempt := 0; attempt < tempFileMaxAttempts; attempt++ {
		f, err := os.CreateTemp(dir, base+".*.tmp")
		if err != nil {
			if errors.Is(err, os.ErrExist) {
				lastErr = err
				continue
			}
			return fmt.Errorf("failed to create temp credentials file: %w", err)
		}

		tmpPath := f.Name()
		committed := false
		defer func() {
			if !committed {
				os.Remove(tmpPath)
			}
		}()

		if err := writeCredentialsSecureCommit(f, tmpPath, path, data); err != nil {
			return err
		}

		committed = true
		return nil
	}

	return fmt.Errorf("every one of %d candidate temp-file names was already taken writing %s: %w", tempFileMaxAttempts, path, lastErr)
}

// writeCredentialsSecureCommit writes data to the already-created temp file
// f, fsyncs it (never just flushes — a rename ordered ahead of the data
// reaching disk can leave a zero-length credentials file after a crash), and
// renames it onto path.
func writeCredentialsSecureCommit(f *os.File, tmpPath, path string, data []byte) error {
	if _, err := f.Write(data); err != nil {
		f.Close()
		return fmt.Errorf("failed to write temp credentials file: %w", err)
	}
	if err := f.Sync(); err != nil {
		f.Close()
		return fmt.Errorf("failed to fsync temp credentials file: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("failed to close temp credentials file: %w", err)
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return fmt.Errorf("failed to rename temp credentials file: %w", err)
	}
	return nil
}

// validateIssuerCredentials rejects a credential record whose numeric fields
// are out of range (C-11). json.Unmarshal already rejects a type mismatch
// (e.g. a string where an int64 is expected) into VorpalCredentialsContent,
// so only the range check is added here.
func validateIssuerCredentials(c VorpalCredentialsContent) error {
	if c.ExpiresIn < 0 {
		return fmt.Errorf("expires_in must not be negative: %d", c.ExpiresIn)
	}
	if c.IssuedAt < 0 {
		return fmt.Errorf("issued_at must not be negative: %d", c.IssuedAt)
	}
	return nil
}

// commitRefreshedCredentials applies a completed exchange to credentials and
// writes the result to path. Returns an error without committing anything
// when the IdP's response is unusable (C-10): an absent expires_in defaults
// to 3600, but an explicit value <= 0 can never satisfy needsRefresh's
// window, which would make every later call rotate again — refused instead.
func commitRefreshedCredentials(
	credentials *VorpalCredentials,
	issuer string,
	path string,
	accessToken string,
	expiresIn *int64,
	issuedAt int64,
	rotatedRefreshToken string,
) error {
	var expires int64
	switch {
	case expiresIn == nil:
		expires = 3600
	case *expiresIn <= 0:
		return fmt.Errorf(
			"OAuth refresh for issuer %s returned a token with a zero lifetime. Please run: vorpal login --issuer %s",
			issuer, issuer,
		)
	default:
		expires = *expiresIn
	}

	issuerCreds, ok := credentials.Issuer[issuer]
	if !ok {
		return fmt.Errorf("no credentials for issuer: %s", issuer)
	}

	issuerCreds.AccessToken = accessToken
	issuerCreds.ExpiresIn = expires
	issuerCreds.IssuedAt = issuedAt
	// Persist rotated refresh token when the IdP returned one; leave the
	// existing value untouched when omitted (some IdPs do not rotate and
	// reuse the original refresh token).
	if rotatedRefreshToken != "" {
		issuerCreds.RefreshToken = rotatedRefreshToken
	}
	credentials.Issuer[issuer] = issuerCreds

	data, err := json.MarshalIndent(credentials, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to serialize credentials: %w", err)
	}

	return writeCredentialsSecure(path, data)
}

// credentialsRefresh serializes the whole critical section — read, refresh
// decision, exchange, write — within this process (C-1), and guards
// credentialsRefreshSpent, the memo of refresh-token digests this process
// has already put on the wire without durably committing a replacement
// (C-3). Held for the whole span, not just the write, so a waiter re-reads
// the winner's committed state instead of acting on its own stale snapshot
// (closing AB-2), and so the memo's check-then-insert is atomic by
// construction.
//
// Scope is this process only: it does not serialize against a separately
// spawned config process, a running vorpal start agent, or the Rust/
// TypeScript SDKs writing the same credentials.json — an accepted residual
// risk shared with Rust (context.rs:1160-1166).
var credentialsRefresh sync.Mutex
var credentialsRefreshSpent = make(map[string]struct{})

// tokenRefresher performs the OAuth refresh-token exchange. Injected so
// tests can count and control exchanges without real network I/O. Contract:
// refresher runs while credentialsRefresh is held and must not call back
// into ClientAuthHeader or clientAuthHeaderAt — sync.Mutex is non-reentrant
// and a re-entrant call deadlocks every authenticated call in the process.
type tokenRefresher func(audience *string, clientId, issuer, refreshToken string) (string, *int64, int64, string, error)

// liveTokenRefresher is the production tokenRefresher: the real IdP
// exchange over the real network.
func liveTokenRefresher(audience *string, clientId, issuer, refreshToken string) (string, *int64, int64, string, error) {
	return refreshAccessToken(audience, clientId, issuer, refreshToken, refreshHTTPTimeout)
}

// clientAuthHeaderAt is the core of ClientAuthHeader, taking the credentials
// path, the refresh operation and a clock as parameters so it is testable
// without touching the real /var/lib/vorpal/key/credentials.json,
// performing network I/O, or depending on the wall clock. Deliberately
// private and not configurable from any production entry point (env var,
// global override) — see ClientAuthHeader.
//
// now is a function, not a plain value, and it is called only after
// credentialsRefresh is held — never before. A value sampled before
// acquiring the lock can predate a still-in-flight winner's later commit; a
// waiter that then compares its stale, pre-lock reading against the
// winner's freshly committed IssuedAt sees a future-dated token and
// refreshes again, once per waiter (C-2, closing AB-2).
func clientAuthHeaderAt(
	credentialsPath string,
	registry string,
	refresher tokenRefresher,
	now func() (int64, error),
) (string, error) {
	credentialsRefresh.Lock()
	defer credentialsRefresh.Unlock()

	// Read here, strictly after the lock — see this function's doc comment.
	nowUnix, err := now()
	if err != nil {
		return "", fmt.Errorf("failed to read clock: %w", err)
	}

	if _, err := os.Stat(credentialsPath); os.IsNotExist(err) {
		// No credentials file - return empty string (optional auth)
		return "", nil
	}

	credentialsData, err := os.ReadFile(credentialsPath)
	if err != nil {
		return "", fmt.Errorf("failed to read credentials file: %w", err)
	}

	var credentials VorpalCredentials
	if err := json.Unmarshal(credentialsData, &credentials); err != nil {
		return "", fmt.Errorf("failed to parse credentials: %w", err)
	}

	registryIssuer, ok := credentials.Registry[registry]
	if !ok {
		// No registry mapping - allow unauthenticated requests
		return "", nil
	}

	issuerCredentials, ok := credentials.Issuer[registryIssuer]
	if !ok {
		return "", fmt.Errorf("no credentials for issuer: %s", registryIssuer)
	}

	if err := validateIssuerCredentials(issuerCredentials); err != nil {
		return "", fmt.Errorf("invalid credentials for issuer %s: %w", registryIssuer, err)
	}

	if needsRefresh(issuerCredentials.IssuedAt, issuerCredentials.ExpiresIn, nowUnix) {
		if issuerCredentials.RefreshToken == "" {
			return "", fmt.Errorf("access token expired and no refresh token available. Please run: vorpal login --issuer %s", registryIssuer)
		}

		// A prior caller may already have put this exact stored token value
		// on the wire. The file it left behind is byte-identical either way,
		// so the memo is the only thing that can tell the two apart.
		refreshTokenDigest := digestHex(issuerCredentials.RefreshToken)

		if _, spent := credentialsRefreshSpent[refreshTokenDigest]; spent {
			return "", fmt.Errorf("OAuth refresh-token exchange already failed for the stored token. Please run: vorpal login --issuer %s", registryIssuer)
		}

		newToken, newExpiresIn, newIssuedAt, newRefreshToken, err := refresher(
			issuerCredentials.Audience,
			issuerCredentials.ClientId,
			registryIssuer,
			issuerCredentials.RefreshToken,
		)
		if err != nil {
			var failure *refreshFailureError
			if errors.As(err, &failure) && failure.kind == refreshFailureSent {
				credentialsRefreshSpent[refreshTokenDigest] = struct{}{}
				return "", spentGrantError(
					registryIssuer,
					fmt.Sprintf("the OAuth refresh-token exchange for issuer %s failed after the token had been sent, so the stored refresh token is no longer usable", registryIssuer),
					failure.err,
				)
			}

			// NotSent, or an unclassified error: the token never left the
			// process, the stored token is untouched, and a later caller may
			// use it.
			return "", fmt.Errorf("failed to refresh token: %w", err)
		}

		// No blocking work is introduced between the exchange above and the
		// commit below: the sequence stays synchronous so the window in
		// which a killed process loses the rotated token to disk (accepted
		// residual risk) does not widen.
		if err := commitRefreshedCredentials(
			&credentials,
			registryIssuer,
			credentialsPath,
			newToken,
			newExpiresIn,
			newIssuedAt,
			newRefreshToken,
		); err != nil {
			// The exchange happened and nothing was committed, so the file
			// still names a token the IdP may have already rotated away.
			// This is the same replay hazard as an outright failure and it
			// is spent for the same reason.
			credentialsRefreshSpent[refreshTokenDigest] = struct{}{}

			return "", spentGrantError(
				registryIssuer,
				fmt.Sprintf("refreshed credentials for issuer %s could not be saved, so the stored refresh token is no longer usable", registryIssuer),
				err,
			)
		}

		issuerCredentials = credentials.Issuer[registryIssuer]
	}

	return fmt.Sprintf("Bearer %s", issuerCredentials.AccessToken), nil
}

// ClientAuthHeader retrieves the authorization header for a given registry.
// Returns the Bearer token string if credentials exist, empty string otherwise, or error on failure.
// This matches the Rust SDK's client_auth_header function.
func ClientAuthHeader(registry string) (string, error) {
	return clientAuthHeaderAt(GetKeyCredentialsPath(), registry, liveTokenRefresher, func() (int64, error) {
		return time.Now().Unix(), nil
	})
}

// getTransportCredentials returns the appropriate gRPC transport credentials
// and host string based on the URI scheme. For https:// addresses, it loads
// the CA certificate (falling back to system trust if unavailable). For http://
// addresses, it uses insecure (plaintext) credentials.
func getTransportCredentials(address string) (credentials.TransportCredentials, string, error) {
	if !strings.HasPrefix(address, "http://") && !strings.HasPrefix(address, "https://") {
		return nil, "", fmt.Errorf("address must start with http:// or https://: %s", address)
	}

	if strings.HasPrefix(address, "https://") {
		host := strings.TrimPrefix(address, "https://")

		tlsConfig := &tls.Config{}

		caCertPath := GetKeyCaPath()
		caCert, err := os.ReadFile(caCertPath)
		if err == nil {
			caCertPool, poolErr := x509.SystemCertPool()
			if poolErr != nil {
				caCertPool = x509.NewCertPool()
			}
			if !caCertPool.AppendCertsFromPEM(caCert) {
				log.Printf("WARNING: CA certificate at %s could not be parsed, falling back to system trust store", caCertPath)
			} else {
				tlsConfig.RootCAs = caCertPool
			}
		}

		return credentials.NewTLS(tlsConfig), host, nil
	}

	host := strings.TrimPrefix(address, "http://")
	return insecure.NewCredentials(), host, nil
}

// BuildClientConn creates a gRPC client connection for the given address.
// It auto-detects the transport based on the URI scheme (http:// = plaintext,
// https:// = TLS with optional CA cert, unix:// = UDS with plaintext).
func BuildClientConn(address string) (*grpc.ClientConn, error) {
	if strings.HasPrefix(address, "unix://") {
		conn, err := grpc.NewClient(
			address,
			grpc.WithTransportCredentials(insecure.NewCredentials()),
		)
		if err != nil {
			return nil, fmt.Errorf("failed to connect to unix socket %s: %w", address, err)
		}
		return conn, nil
	}

	creds, host, err := getTransportCredentials(address)
	if err != nil {
		return nil, fmt.Errorf("failed to configure transport for %s: %w", address, err)
	}

	conn, err := grpc.NewClient(
		host,
		grpc.WithTransportCredentials(creds),
	)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to %s: %w", address, err)
	}

	return conn, nil
}

func GetContext() *ConfigContext {
	cmd, err := NewCommand()
	if err != nil {
		log.Fatal(err)
	}

	store := ConfigContextStore{
		artifact:           make(map[string]*artifact.Artifact),
		artifactInputCache: make(map[string]string),
		variable:           cmd.ArtifactVariable,
	}

	system, err := GetSystem(cmd.ArtifactSystem)
	if err != nil {
		log.Fatalf("failed to get system: %v", err)
	}

	// Auth headers will be added per-request using ClientAuthHeader

	clientConnAgent, err := BuildClientConn(cmd.Agent)
	if err != nil {
		log.Fatalf("failed to connect to agent: %v", err)
	}

	clientConnArtifact, err := BuildClientConn(cmd.Registry)
	if err != nil {
		log.Fatalf("failed to connect to registry: %v", err)
	}

	return &ConfigContext{
		artifact:          cmd.Artifact,
		artifactContext:   cmd.ArtifactContext,
		artifactNamespace: cmd.ArtifactNamespace,
		artifactSystem:    *system,
		artifactSystemStr: cmd.ArtifactSystem,
		artifactUnlock:    cmd.ArtifactUnlock,
		clientAgent:       agent.NewAgentServiceClient(clientConnAgent),
		clientArtifact:    artifact.NewArtifactServiceClient(clientConnArtifact),
		port:              cmd.Port,
		registry:          cmd.Registry,
		store:             store,
	}
}

func (c *ConfigContext) AddArtifact(artifact *artifact.Artifact) (*string, error) {
	if artifact.Name == "" {
		return nil, fmt.Errorf("'name' is required")
	}

	if len(artifact.Steps) == 0 {
		return nil, fmt.Errorf("'steps' is required")
	}

	if len(artifact.Systems) == 0 {
		return nil, fmt.Errorf("'systems' is required")
	}

	// Validate target is in systems list
	targetSupported := false
	for _, s := range artifact.Systems {
		if s == artifact.Target {
			targetSupported = true
			break
		}
	}
	if !targetSupported {
		return nil, fmt.Errorf(
			"artifact '%s' does not support system '%s' (supported: %v)",
			artifact.Name,
			artifact.Target.String(),
			artifact.Systems,
		)
	}

	artifactJson, err := SerializeArtifactJSON(artifact)
	if err != nil {
		return nil, fmt.Errorf("failed to serialize artifact for digest: %w", err)
	}

	artifactDigest := fmt.Sprintf("%x", sha256.Sum256(artifactJson))

	if _, ok := c.store.artifact[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	// Check the input-to-output digest cache for deduplication.
	// The input digest (computed from un-hydrated sources) differs from the
	// output digest (computed after the agent hydrates source digests), so
	// we maintain a mapping from input -> output to short-circuit repeated
	// calls for the same logical artifact.
	if outputDigest, ok := c.store.artifactInputCache[artifactDigest]; ok {
		if _, exists := c.store.artifact[outputDigest]; exists {
			return &outputDigest, nil
		}
	}

	// Preserve the input digest before it gets reassigned to the response digest
	inputDigest := artifactDigest

	// TODO: make this run in parallel

	prepareRequest := &agent.PrepareArtifactRequest{
		Artifact:          artifact,
		ArtifactContext:   c.artifactContext,
		ArtifactNamespace: c.artifactNamespace,
		ArtifactUnlock:    c.artifactUnlock,
		Registry:          c.registry,
	}

	// Get auth header for this registry
	authHeader, err := ClientAuthHeader(c.registry)
	if err != nil {
		return nil, fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	clientResponse, err := c.clientAgent.PrepareArtifact(ctx, prepareRequest)
	if err != nil {
		return nil, fmt.Errorf("error preparing artifact: %v", err)
	}

	for {
		response, err := clientResponse.Recv()
		if err == io.EOF {
			break
		}

		if err != nil {
			return nil, fmt.Errorf("error receiving response: %v", err)
		}

		if response.ArtifactOutput != nil {
			output := fmt.Sprintf("%s |> %s", artifact.Name, *response.ArtifactOutput)
			println(output)
		}

		if response.Artifact != nil {
			artifact = response.Artifact
		}

		if response.ArtifactDigest != nil {
			artifactDigest = *response.ArtifactDigest
		}
	}

	if _, ok := c.store.artifact[artifactDigest]; !ok {
		c.store.artifact[artifactDigest] = artifact
	}

	// Map the input digest to the output digest so subsequent calls with
	// the same un-hydrated artifact return immediately.
	c.store.artifactInputCache[inputDigest] = artifactDigest

	return &artifactDigest, nil
}

func fetchArtifacts(client artifact.ArtifactServiceClient, digest string, namespace string, store map[string]*artifact.Artifact, registry string) error {
	if _, ok := store[digest]; ok {
		return nil
	}

	// Get auth header
	authHeader, err := ClientAuthHeader(registry)
	if err != nil {
		return fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	clientResponse, err := client.GetArtifact(ctx, &artifact.ArtifactRequest{Digest: digest, Namespace: namespace})
	if err != nil {
		return fmt.Errorf("error fetching artifact: %v", err)
	}

	if _, ok := store[digest]; !ok {
		store[digest] = clientResponse
	}

	for _, step := range clientResponse.Steps {
		if step != nil {
			for _, digest := range step.Artifacts {
				if err := fetchArtifacts(client, digest, namespace, store, registry); err != nil {
					return err
				}
			}
		}
	}

	return nil
}

// isValidComponent returns true if s is non-empty and every character is in
// the allowed set for alias components: alphanumeric (a-z, A-Z, 0-9), hyphens
// (-), dots (.), underscores (_), and plus signs (+).
func isValidComponent(s string) bool {
	if len(s) == 0 {
		return false
	}
	for _, c := range s {
		if !((c >= 'a' && c <= 'z') ||
			(c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') ||
			c == '-' || c == '.' || c == '_' || c == '+') {
			return false
		}
	}
	return true
}

// parseArtifactAlias parses an artifact alias into its components.
// Format: [<namespace>/]<name>[:<tag>]
// - namespace is optional (defaults to "library")
// - tag is optional (defaults to "latest")
// - name is required
func parseArtifactAlias(alias string) (*ArtifactAlias, error) {
	// Validate input
	if alias == "" {
		return nil, fmt.Errorf("alias cannot be empty")
	}

	if len(alias) > 255 {
		return nil, fmt.Errorf("alias too long (max 255 characters)")
	}

	// Step 1: Extract tag (split on rightmost ':')
	tag := ""
	base := alias

	if lastColon := strings.LastIndex(alias, ":"); lastColon != -1 {
		tagPart := alias[lastColon+1:]
		if tagPart == "" {
			return nil, fmt.Errorf("tag cannot be empty")
		}
		tag = tagPart
		base = alias[:lastColon]
	}

	// Step 2: Extract namespace/name (split on '/')
	namespace := ""
	name := ""

	slashCount := strings.Count(base, "/")

	switch slashCount {
	case 0:
		// Just name
		name = base
	case 1:
		// namespace/name
		slashIdx := strings.Index(base, "/")
		namespace = base[:slashIdx]
		name = base[slashIdx+1:]

		if namespace == "" {
			return nil, fmt.Errorf("namespace cannot be empty")
		}
	default:
		// Too many slashes
		return nil, fmt.Errorf("invalid format: too many path separators")
	}

	if name == "" {
		return nil, fmt.Errorf("name is required")
	}

	// Step 3: Validate component characters
	if !isValidComponent(name) {
		return nil, fmt.Errorf("name contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)")
	}

	if namespace != "" && !isValidComponent(namespace) {
		return nil, fmt.Errorf("namespace contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)")
	}

	if tag != "" && !isValidComponent(tag) {
		return nil, fmt.Errorf("tag contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)")
	}

	// Step 4: Apply defaults
	if tag == "" {
		tag = "latest"
	}

	if namespace == "" {
		namespace = "library"
	}

	return &ArtifactAlias{
		Name:      name,
		Namespace: namespace,
		Tag:       tag,
	}, nil
}

func (c *ConfigContext) FetchArtifactAlias(alias string) (*string, error) {
	parsed, err := parseArtifactAlias(alias)
	if err != nil {
		return nil, fmt.Errorf("failed to parse artifact alias: %w", err)
	}

	request := &artifact.GetArtifactAliasRequest{
		Name:      parsed.Name,
		Namespace: parsed.Namespace,
		System:    c.artifactSystem,
		Tag:       parsed.Tag,
	}

	// Get auth header for this registry
	authHeader, err := ClientAuthHeader(c.registry)
	if err != nil {
		return nil, fmt.Errorf("failed to get auth header: %w", err)
	}

	// Create context with auth header if present
	ctx := context.Background()
	if authHeader != "" {
		ctx = metadata.AppendToOutgoingContext(ctx, "authorization", authHeader)
	}

	response, err := c.clientArtifact.GetArtifactAlias(ctx, request)
	if err != nil {
		return nil, fmt.Errorf("error fetching artifact alias: %v", err)
	}

	artifactDigest := response.Digest

	if _, ok := c.store.artifact[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	err = fetchArtifacts(c.clientArtifact, artifactDigest, parsed.Namespace, c.store.artifact, c.registry)
	if err != nil {
		return nil, fmt.Errorf("error fetching '%s': %v", artifactDigest, err)
	}

	return &artifactDigest, nil
}

func (c *ConfigContext) GetArtifact(digest string) *artifact.Artifact {
	return c.store.artifact[digest]
}

func (c *ConfigContext) GetArtifactContextPath() string {
	return c.artifactContext
}

func (c *ConfigContext) GetArtifactName() string {
	return c.artifact
}

func (c *ConfigContext) GetArtifactNamespace() string {
	return c.artifactNamespace
}

func (c *ConfigContext) GetTarget() artifact.ArtifactSystem {
	return c.artifactSystem
}

// GetTargetStr returns the canonical "arch-os" spelling of the resolved
// target system (e.g. "aarch64-darwin"), as validated by GetSystem.
func (c *ConfigContext) GetTargetStr() string {
	return c.artifactSystemStr
}

func (c *ConfigContext) GetVariable(name string) *string {
	if _, ok := c.store.variable[name]; !ok {
		return nil
	}

	value := c.store.variable[name]

	return &value
}

func (c *ConfigContext) Run() error {
	var grpcServerOpts []grpc.ServerOption

	grpcServer := grpc.NewServer(grpcServerOpts...)

	apiContext.RegisterContextServiceServer(grpcServer, NewConfigServer(c.store))

	listenerAddr := fmt.Sprintf("[::]:%d", c.port)

	listener, err := net.Listen("tcp", listenerAddr)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	log.Printf("context service: %s", listenerAddr)

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}
