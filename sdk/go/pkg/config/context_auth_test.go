package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

func fixedClock(now int64) func() (int64, error) {
	return func() (int64, error) { return now, nil }
}

func credentialsFixture(issuer, refreshToken string, issuedAt, expiresIn int64, accessToken string) VorpalCredentials {
	return VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			issuer: {
				AccessToken:  accessToken,
				ClientId:     "test-client-id",
				ExpiresIn:    expiresIn,
				IssuedAt:     issuedAt,
				RefreshToken: refreshToken,
				Scopes:       []string{"read", "write"},
			},
		},
		Registry: map[string]string{
			"https://registry.example.com": issuer,
		},
	}
}

func writeFixture(t *testing.T, path string, creds VorpalCredentials) {
	t.Helper()
	data, err := json.MarshalIndent(creds, "", "  ")
	if err != nil {
		t.Fatalf("failed to marshal fixture: %v", err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("failed to write fixture: %v", err)
	}
}

func readPersisted(t *testing.T, path string) VorpalCredentials {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("failed to read persisted credentials: %v", err)
	}
	var creds VorpalCredentials
	if err := json.Unmarshal(data, &creds); err != nil {
		t.Fatalf("failed to parse persisted credentials: %v", err)
	}
	return creds
}

func noRefresherExpected(t *testing.T) tokenRefresher {
	return func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		t.Fatal("refresher must not be called")
		return "", nil, 0, "", nil
	}
}

// resetSpentTokenMemo clears the package-global spent-refresh-token memo
// (C-3) via t.Cleanup, so every test that exercises a refresh starts from
// an empty memo regardless of what an earlier test in the same binary
// spent. Without this, `go test -count=2` (or any test ordering that
// revisits the same digest) fails: the memo is process-global by design
// and outlives any single test.
func resetSpentTokenMemo(t *testing.T) {
	t.Helper()
	t.Cleanup(func() {
		credentialsRefreshState.mu.Lock()
		defer credentialsRefreshState.mu.Unlock()
		credentialsRefreshState.spent = make(map[string]struct{})
	})
}

// ---------------------------------------------------------------------------
// Basic lookup behavior (AC 1, unchanged shape, ported to the new seam — C-12)
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtNoFile(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	header, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(time.Now().Unix()))
	if err != nil {
		t.Fatalf("expected no error when credentials file doesn't exist, got: %v", err)
	}
	if header != "" {
		t.Fatalf("expected empty header when credentials file doesn't exist, got: %q", header)
	}
}

func TestClientAuthHeaderAtValid(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("example-issuer", "test-refresh-token", now, 3600, "test-access-token-12345"))

	header, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(now))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if expected := "Bearer test-access-token-12345"; header != expected {
		t.Fatalf("expected header %q, got %q", expected, header)
	}
}

func TestClientAuthHeaderAtRegistryNotFound(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	creds := credentialsFixture("example-issuer", "test-refresh-token", now, 3600, "test-access-token")
	creds.Registry = map[string]string{"https://other-registry.example.com": "example-issuer"}
	writeFixture(t, credPath, creds)

	header, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(now))
	if err != nil {
		t.Fatalf("expected no error for registry not found, got: %v", err)
	}
	if header != "" {
		t.Fatalf("expected empty header for registry not found, got: %q", header)
	}
}

func TestClientAuthHeaderAtIssuerNotFound(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	creds := credentialsFixture("different-issuer", "test-refresh-token", now, 3600, "test-access-token")
	creds.Registry = map[string]string{"https://registry.example.com": "missing-issuer"}
	writeFixture(t, credPath, creds)

	_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(now))
	if err == nil {
		t.Fatal("expected error for issuer not found, got nil")
	}
	if !strings.Contains(err.Error(), "no credentials for issuer") {
		t.Fatalf("expected error naming missing issuer, got %q", err.Error())
	}
}

func TestClientAuthHeaderAtInvalidJSON(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	if err := os.WriteFile(credPath, []byte(`{"invalid": json}`), 0o644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(time.Now().Unix()))
	if err == nil {
		t.Fatal("expected error for invalid JSON, got nil")
	}
	if !strings.Contains(err.Error(), "failed to parse credentials") {
		t.Fatalf("expected parse error, got %q", err.Error())
	}
}

func TestClientAuthHeaderAtMultipleRegistries(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()

	creds := VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"issuer-one": {AccessToken: "token-one", ClientId: "client-one", ExpiresIn: 3600, IssuedAt: now, RefreshToken: "refresh-one", Scopes: []string{"read"}},
			"issuer-two": {AccessToken: "token-two", ClientId: "client-two", ExpiresIn: 7200, IssuedAt: now, RefreshToken: "refresh-two", Scopes: []string{"read", "write"}},
		},
		Registry: map[string]string{
			"https://registry1.example.com": "issuer-one",
			"https://registry2.example.com": "issuer-two",
		},
	}
	writeFixture(t, credPath, creds)

	header1, err := clientAuthHeaderAt(credPath, "https://registry1.example.com", noRefresherExpected(t), fixedClock(now))
	if err != nil {
		t.Fatalf("unexpected error for registry1: %v", err)
	}
	if header1 != "Bearer token-one" {
		t.Fatalf("expected 'Bearer token-one', got %q", header1)
	}

	header2, err := clientAuthHeaderAt(credPath, "https://registry2.example.com", noRefresherExpected(t), fixedClock(now))
	if err != nil {
		t.Fatalf("unexpected error for registry2: %v", err)
	}
	if header2 != "Bearer token-two" {
		t.Fatalf("expected 'Bearer token-two', got %q", header2)
	}
}

// TestClientAuthHeaderWiresProductionPathRefresherAndClock exercises the
// exported ClientAuthHeader wrapper itself (VPL-189-CL26: it had no test at
// all after the path-override test seam was removed by C-12). C-12
// deliberately leaves no way to redirect ClientAuthHeader off the real
// /var/lib/vorpal/key/credentials.json, so this can only assert the one
// branch reachable without that file existing on the test host: "no
// credentials file" returns an empty header and no error, exactly like
// clientAuthHeaderAt's equivalent branch, proving the production wiring
// reaches that shared core rather than diverging from it.
func TestClientAuthHeaderWiresProductionPathRefresherAndClock(t *testing.T) {
	if _, err := os.Stat(GetKeyCredentialsPath()); err == nil {
		t.Skip("a real credentials file exists on this host; skipping to avoid depending on its contents")
	}

	header, err := ClientAuthHeader("https://registry.example.com")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if header != "" {
		t.Fatalf("expected empty header when no credentials file exists, got %q", header)
	}
}

func TestGetKeyCredentialsPath(t *testing.T) {
	rootDir := GetRootDirPath()
	if rootDir != "/var/lib/vorpal" {
		t.Fatalf("expected root dir '/var/lib/vorpal', got %q", rootDir)
	}

	keyDir := GetRootKeyDirPath()
	expected := filepath.Join("/var/lib/vorpal", "key")
	if keyDir != expected {
		t.Fatalf("expected key dir %q, got %q", expected, keyDir)
	}

	credPath := GetKeyCredentialsPath()
	expected = filepath.Join("/var/lib/vorpal", "key", "credentials.json")
	if credPath != expected {
		t.Fatalf("expected credentials path %q, got %q", expected, credPath)
	}
}

// ---------------------------------------------------------------------------
// End-to-end refresh over a real OIDC test server (AC 1(b), rotation shape)
// ---------------------------------------------------------------------------

// newOIDCRefreshTestServer returns an httptest server that serves an OIDC
// discovery document at /.well-known/openid-configuration and a token
// endpoint at /token returning the supplied JSON body. httptest binds to
// 127.0.0.1, so it satisfies C-8's loopback exemption for plaintext HTTP.
func newOIDCRefreshTestServer(t *testing.T, tokenResponseBody string) *httptest.Server {
	t.Helper()

	mux := http.NewServeMux()
	var server *httptest.Server

	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})

	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		if got := r.Form.Get("grant_type"); got != "refresh_token" {
			http.Error(w, fmt.Sprintf("unexpected grant_type %q", got), http.StatusBadRequest)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, tokenResponseBody)
	})

	server = httptest.NewServer(mux)
	return server
}

func TestClientAuthHeaderAtRefreshPersistsRotatedToken(t *testing.T) {
	resetSpentTokenMemo(t)
	tokenBody := `{
		"access_token": "new-access-token",
		"expires_in": 3600,
		"refresh_token": "new-refresh-token-rotated"
	}`
	server := newOIDCRefreshTestServer(t, tokenBody)
	defer server.Close()

	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	expiredAt := time.Now().Unix() - 7200
	creds := credentialsFixture(server.URL, "old-refresh-token", expiredAt, 3600, "old-access-token")
	writeFixture(t, credPath, creds)

	header, err := clientAuthHeaderAt(credPath, "https://registry.example.com", liveTokenRefresher, fixedClock(time.Now().Unix()))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if expected := "Bearer new-access-token"; header != expected {
		t.Fatalf("expected header %q, got %q", expected, header)
	}

	persisted := readPersisted(t, credPath)
	got, ok := persisted.Issuer[server.URL]
	if !ok {
		t.Fatalf("issuer %q missing from persisted credentials", server.URL)
	}
	if got.AccessToken != "new-access-token" {
		t.Errorf("expected access_token rewritten, got %q", got.AccessToken)
	}
	if got.RefreshToken != "new-refresh-token-rotated" {
		t.Errorf("expected refresh_token rotated, got %q", got.RefreshToken)
	}
	if got.ClientId != "test-client-id" {
		t.Errorf("client_id should be preserved, got %q", got.ClientId)
	}
}

func TestClientAuthHeaderAtRefreshPreservesRefreshTokenWhenOmitted(t *testing.T) {
	resetSpentTokenMemo(t)
	tokenBody := `{"access_token": "new-access-token", "expires_in": 3600}`
	server := newOIDCRefreshTestServer(t, tokenBody)
	defer server.Close()

	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	expiredAt := time.Now().Unix() - 7200
	writeFixture(t, credPath, credentialsFixture(server.URL, "original-refresh-token", expiredAt, 3600, "old-access-token"))

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", liveTokenRefresher, fixedClock(time.Now().Unix())); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	persisted := readPersisted(t, credPath)
	got := persisted.Issuer[server.URL]
	if got.RefreshToken != "original-refresh-token" {
		t.Fatalf("expected refresh_token preserved when IdP omits rotation, got %q", got.RefreshToken)
	}
	if got.AccessToken != "new-access-token" {
		t.Errorf("expected access_token rewritten, got %q", got.AccessToken)
	}
}

// ---------------------------------------------------------------------------
// C-1 / C-2 — serialization and post-lock clock sampling (AB-1, AB-2)
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtSerializesConcurrentRefresh(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-concurrent", now-7200, 3600, "old-access-token"))

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		atomic.AddInt32(&calls, 1)
		expires := int64(3600)
		return "new-access-token", &expires, time.Now().Unix(), "", nil
	}
	clock := func() (int64, error) { return time.Now().Unix(), nil }

	const n = 8
	var wg sync.WaitGroup
	errs := make([]error, n)
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
			errs[i] = err
		}(i)
	}
	wg.Wait()

	for i, err := range errs {
		if err != nil {
			t.Fatalf("goroutine %d: unexpected error: %v", i, err)
		}
	}

	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("expected exactly one refresh exchange across %d concurrent callers, got %d", n, got)
	}
}

// ---------------------------------------------------------------------------
// C-3 / C-4 — spent-token memo and NotSent/Sent classification (AB-1, AB-3, AB-4)
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtNeverReplaysATokenAfterSentFailure(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-sent-fail", now-7200, 3600, "old-access-token"))

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		atomic.AddInt32(&calls, 1)
		return "", nil, 0, "", sentErr(fmt.Errorf("token endpoint returned 500"))
	}
	clock := fixedClock(now)

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock); err == nil {
		t.Fatal("expected error from first call")
	}

	_, err2 := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
	if err2 == nil {
		t.Fatal("expected error from second call")
	}
	if !strings.Contains(err2.Error(), "already failed") {
		t.Fatalf("expected replay-guard error, got: %v", err2)
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("expected exactly one refresh exchange, got %d", got)
	}
}

func TestClientAuthHeaderAtRetriesARefreshThatNeverReachedTheIdp(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-not-sent", now-7200, 3600, "old-access-token"))

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		n := atomic.AddInt32(&calls, 1)
		if n == 1 {
			return "", nil, 0, "", notSentErr(fmt.Errorf("dns lookup failed"))
		}
		expires := int64(3600)
		return "new-access-token", &expires, time.Now().Unix(), "", nil
	}
	clock := fixedClock(now)

	err1 := func() error {
		_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
		return err
	}()
	if err1 == nil {
		t.Fatal("expected error from first call")
	}
	if strings.Contains(err1.Error(), "already failed") {
		t.Fatalf("a NotSent failure must not spend the token: %v", err1)
	}

	header, err2 := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
	if err2 != nil {
		t.Fatalf("expected second call to succeed once the local fault clears, got: %v", err2)
	}
	if header != "Bearer new-access-token" {
		t.Fatalf("got %q", header)
	}
}

func TestClientAuthHeaderAtNeverReplaysATokenWhoseRefreshFailedToPersist(t *testing.T) {
	resetSpentTokenMemo(t)
	if os.Geteuid() == 0 {
		t.Skip("running as root: permission-based write denial is not enforced")
	}

	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-persist-fail", now-7200, 3600, "old-access-token"))

	if err := os.Chmod(tempDir, 0o500); err != nil {
		t.Fatalf("failed to lock down temp dir: %v", err)
	}
	t.Cleanup(func() { os.Chmod(tempDir, 0o700) })

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		atomic.AddInt32(&calls, 1)
		expires := int64(3600)
		return "new-access-token", &expires, time.Now().Unix(), "", nil
	}
	clock := fixedClock(now)

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock); err == nil {
		t.Fatal("expected persist failure to error")
	}

	if err := os.Chmod(tempDir, 0o700); err != nil {
		t.Fatalf("failed to restore temp dir permissions: %v", err)
	}

	entries, err := os.ReadDir(tempDir)
	if err != nil {
		t.Fatalf("failed to list temp dir: %v", err)
	}
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".tmp") {
			t.Fatalf("leftover temp credentials file after a failed write: %s", e.Name())
		}
	}

	_, err2 := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
	if err2 == nil {
		t.Fatal("expected second call to fail: the token was spent by the persist failure")
	}
	if !strings.Contains(err2.Error(), "already failed") {
		t.Fatalf("expected replay-guard error, got: %v", err2)
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("expected exactly one exchange (second call must not re-exchange), got %d", got)
	}
}

// ---------------------------------------------------------------------------
// C-5 — atomic, mode-enforcing write (AB-5, AB-6)
// ---------------------------------------------------------------------------

func TestWriteCredentialsSecureOverwritesExistingFileMode(t *testing.T) {
	tempDir := t.TempDir()
	path := filepath.Join(tempDir, "credentials.json")
	if err := os.WriteFile(path, []byte(`{"issuer":{},"registry":{}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := writeCredentialsSecure(path, []byte(`{"issuer":{},"registry":{},"n":1}`)); err != nil {
		t.Fatalf("writeCredentialsSecure: %v", err)
	}

	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("expected mode 0600 after rewrite of a pre-existing 0644 file, got %o", info.Mode().Perm())
	}
}

// TestWriteCredentialsSecureUnlinksTempFileOnRenameFailure directly tests
// AB-13's failure window: a fault AFTER the temp file is created but BEFORE
// the rename commits. The dir-permission trick used elsewhere in this file
// (chmod 0500) blocks os.CreateTemp itself, so its "no leftover .tmp"
// assertion is vacuous — no temp file is ever created for it to leak
// (VPL-189-CL20/F5). Making the destination a pre-existing directory lets
// the temp file be created normally and fails only at os.Rename, isolating
// the window the control is actually meant to close.
func TestWriteCredentialsSecureUnlinksTempFileOnRenameFailure(t *testing.T) {
	tempDir := t.TempDir()
	path := filepath.Join(tempDir, "credentials.json")
	if err := os.Mkdir(path, 0o700); err != nil {
		t.Fatalf("failed to create directory at destination path: %v", err)
	}

	err := writeCredentialsSecure(path, []byte(`{"issuer":{},"registry":{}}`))
	if err == nil {
		t.Fatal("expected writeCredentialsSecure to fail when the destination is a directory")
	}

	entries, err := os.ReadDir(tempDir)
	if err != nil {
		t.Fatalf("failed to list temp dir: %v", err)
	}
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".tmp") {
			t.Fatalf("leftover temp credentials file after a failed rename: %s", e.Name())
		}
	}
}

func TestWriteCredentialsSecureReaderDuringWriter(t *testing.T) {
	tempDir := t.TempDir()
	path := filepath.Join(tempDir, "credentials.json")
	if err := os.WriteFile(path, []byte(`{"issuer":{},"registry":{}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	var stop int32
	var readErr error
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for atomic.LoadInt32(&stop) == 0 {
			data, err := os.ReadFile(path)
			if err != nil {
				// A brief rename window can make the path momentarily
				// unreadable; that is not a torn read.
				continue
			}
			var v map[string]interface{}
			if jsonErr := json.Unmarshal(data, &v); jsonErr != nil {
				readErr = fmt.Errorf("reader observed a torn/partial credentials file: %v", jsonErr)
				return
			}
		}
	}()

	for i := 0; i < 200; i++ {
		payload := fmt.Sprintf(`{"issuer":{},"registry":{},"n":%d}`, i)
		if err := writeCredentialsSecure(path, []byte(payload)); err != nil {
			t.Fatalf("writeCredentialsSecure: %v", err)
		}
	}

	atomic.StoreInt32(&stop, 1)
	wg.Wait()

	if readErr != nil {
		t.Fatal(readErr)
	}
}

// ---------------------------------------------------------------------------
// C-6 / C-7 — skew-safe age and proportional window (AB-7, AB-8)
// ---------------------------------------------------------------------------

func TestNeedsRefreshTreatsFutureIssuedAtAsUnknownAge(t *testing.T) {
	now := int64(1_000_000)
	if !needsRefresh(now+3600, 3600, now) {
		t.Fatal("a future-dated issued_at must be treated as due for refresh, not clamped to zero age")
	}
}

func TestNeedsRefreshProportionalWindow(t *testing.T) {
	issuedAt := int64(1_000_000)

	for _, expiresIn := range []int64{1, 2, 60, 299, 300, 301, 600, 3600} {
		t.Run(fmt.Sprintf("expiresIn=%d", expiresIn), func(t *testing.T) {
			window := expiresIn / 2
			if window > 300 {
				window = 300
			}

			if needsRefresh(issuedAt, expiresIn, issuedAt) {
				t.Fatal("a token issued now should never be due for refresh")
			}

			boundaryNow := issuedAt + expiresIn - window
			if !needsRefresh(issuedAt, expiresIn, boundaryNow) {
				t.Fatalf("token should be due for refresh at its window boundary (now=%d)", boundaryNow)
			}
		})
	}
}

func TestClientAuthHeaderAtRefreshesDespiteFutureIssuedAt(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-future", now+3600, 3600, "old-access-token"))

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		atomic.AddInt32(&calls, 1)
		expires := int64(3600)
		return "new-access-token", &expires, time.Now().Unix(), "", nil
	}
	clock := fixedClock(now)

	header, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, clock)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if header != "Bearer new-access-token" {
		t.Fatalf("got %q", header)
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("expected exactly one exchange, got %d", got)
	}
}

// TestClientAuthHeaderAtSamplesClockAfterLock is C-2's own test (VPL-189-CL22):
// TestClientAuthHeaderAtSerializesConcurrentRefresh above proves exactly one
// exchange happens, but with a fast refresher and near-identical real
// timestamps it would pass even if now() were sampled before Lock() — the
// clock deltas involved are negligible either way. Here the winning
// refresher is deliberately slow, so a pre-lock sample would be
// observably stale by the time a waiter's turn comes: it would predate the
// winner's freshly committed IssuedAt, needsRefresh's future-dated-IssuedAt
// rule (C-6) would read that as due, and every waiter would refresh again.
//
// The delay is just over one second, not 50ms (round 1's testing judge
// reproduced the 50ms version passing 56/60 runs under the exact mutation
// it exists to catch): both now() and IssuedAt are truncated to whole
// seconds by time.Time.Unix(), so a 50ms gap only crosses a second
// boundary by chance — most runs sample and commit inside the same integer
// second regardless of ordering, and the mutant is caught only when they
// happen to straddle one. Adding any duration >= 1s to a real number always
// advances its floor by at least 1 (floor(x+1) = floor(x)+1 for every x),
// so a >1s delay makes the pre-lock/post-lock second value differ on every
// run, deterministically, rather than on the ~5% of runs that happened to
// cross a boundary.
func TestClientAuthHeaderAtSamplesClockAfterLock(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-clock-after-lock", now-7200, 3600, "old-access-token"))

	var calls int32
	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		atomic.AddInt32(&calls, 1)
		time.Sleep(1050 * time.Millisecond)
		expires := int64(3600)
		return "new-access-token", &expires, time.Now().Unix(), "", nil
	}
	wallClock := func() (int64, error) { return time.Now().Unix(), nil }

	const n = 8
	var wg sync.WaitGroup
	errs := make([]error, n)
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, wallClock)
			errs[i] = err
		}(i)
	}
	wg.Wait()

	for i, err := range errs {
		if err != nil {
			t.Fatalf("goroutine %d: unexpected error: %v", i, err)
		}
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("expected exactly one refresh exchange despite the slow winner, got %d", got)
	}
}

// ---------------------------------------------------------------------------
// C-8 — egress origin validation (AB-9, AB-10 discovery arm)
// ---------------------------------------------------------------------------

func TestCredentialEgressOriginRefusesNonHTTPS(t *testing.T) {
	if _, err := credentialEgressOrigin("http://idp.example.com"); err == nil {
		t.Fatal("expected error for a non-loopback http issuer")
	}
}

func TestCredentialEgressOriginAllowsLoopbackHTTP(t *testing.T) {
	origin, err := credentialEgressOrigin("http://127.0.0.1:8080")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if origin != "http://127.0.0.1:8080" {
		t.Fatalf("got %q", origin)
	}
}

func TestCredentialEgressOriginNormalizesDefaultPort(t *testing.T) {
	origin, err := credentialEgressOrigin("https://idp.example.com")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if origin != "https://idp.example.com:443" {
		t.Fatalf("got %q", origin)
	}
}

func TestRefreshAccessTokenRefusesNonHTTPSIssuer(t *testing.T) {
	_, _, _, _, err := refreshAccessToken(nil, "client", "http://idp.example.com", "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureNotSent {
		t.Fatalf("expected NotSent classification, got: %v", err)
	}
}

func TestRefreshAccessTokenRefusesMismatchedTokenEndpointOrigin(t *testing.T) {
	var otherCalled int32
	other := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&otherCalled, 1)
	}))
	defer other.Close()

	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, other.URL+"/token")
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error for mismatched token_endpoint origin")
	}
	if atomic.LoadInt32(&otherCalled) != 0 {
		t.Fatal("token endpoint on a different origin must never be called")
	}
}

func TestRefreshAccessTokenDoesNotDoubleSlashForTrailingSlashIssuer(t *testing.T) {
	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"access_token": "new-access-token", "expires_in": 3600}`)
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	accessToken, _, _, _, err := refreshAccessToken(nil, "client", server.URL+"/", "refresh-token", time.Second)
	if err != nil {
		t.Fatalf("unexpected error for a trailing-slash issuer: %v", err)
	}
	if accessToken != "new-access-token" {
		t.Fatalf("got %q", accessToken)
	}
}

func TestCredentialEgressOriginIsHostCaseInsensitive(t *testing.T) {
	origin, err := credentialEgressOrigin("https://IdP.Example.COM")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if origin != "https://idp.example.com:443" {
		t.Fatalf("expected lowercased host, got %q", origin)
	}
}

// ---------------------------------------------------------------------------
// C-4 — refreshAccessToken's real Sent/NotSent classification, exercised
// through actual HTTP round trips rather than fabricated by a mock
// refresher (VPL-189-CL16: the replay-guard tests above prove
// clientAuthHeaderAt honors a classification it is handed, not that
// refreshAccessToken produces the right one).
// ---------------------------------------------------------------------------

func TestRefreshAccessTokenClassifiesTokenEndpointStatusAsSent(t *testing.T) {
	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "server_error", http.StatusInternalServerError)
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureSent {
		t.Fatalf("expected Sent classification for a token-endpoint 500, got: %v", err)
	}
}

// TestRefreshAccessTokenClassifiesTransportFailureAsSent exercises the
// PostForm error return itself (context.go's `client.PostForm(...)` failing
// directly), not a token endpoint that responds with a bad status — the two
// prior Sent tests above both get a real HTTP response and fail on its
// content. Hijacking and closing the connection with no response at all
// forces a transport-level error out of PostForm, the arm VPL-189-CL37
// found untested (VPL-189-CL37, VPL-189-G2).
func TestRefreshAccessTokenClassifiesTransportFailureAsSent(t *testing.T) {
	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		hj, ok := w.(http.Hijacker)
		if !ok {
			t.Fatal("test server's ResponseWriter does not support hijacking")
		}
		conn, _, err := hj.Hijack()
		if err != nil {
			t.Fatalf("failed to hijack connection: %v", err)
		}
		// Close with no response at all: the client sees a transport
		// failure (EOF/connection reset) out of PostForm itself, not a
		// parsed HTTP response with an error status.
		conn.Close()
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error when the token endpoint connection is closed with no response")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureSent {
		t.Fatalf("expected Sent classification for a transport failure on the token request, got: %v", err)
	}
}

func TestRefreshAccessTokenClassifiesMalformedTokenResponseAsSent(t *testing.T) {
	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, "not json")
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureSent {
		t.Fatalf("expected Sent classification for an unparseable token response, got: %v", err)
	}
}

// ---------------------------------------------------------------------------
// C-3 — an empty access_token in a 200 response must not destroy the
// working credential (VPL-189-CL3).
// ---------------------------------------------------------------------------

func TestRefreshAccessTokenRefusesEmptyAccessToken(t *testing.T) {
	mux := http.NewServeMux()
	var server *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, server.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"access_token": "", "expires_in": 3600}`)
	})
	server = httptest.NewServer(mux)
	defer server.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error for an empty access_token")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureSent {
		t.Fatalf("expected Sent classification, got: %v", err)
	}
}

// TestClientAuthHeaderAtPreservesOldAccessTokenOnSentFailure exercises
// clientAuthHeaderAt's own decision — leave the stored credential untouched
// when a Sent-classified refresh failure is handed to it — using a mock
// refresher that hands back the same failure refreshAccessToken produces for
// an empty access_token. It does not itself decide to refuse an empty
// access_token; TestRefreshAccessTokenRefusesEmptyAccessToken above is what
// exercises that decision, in refreshAccessToken (VPL-189-CL41: this test
// was previously named for the wrong function's decision).
func TestClientAuthHeaderAtPreservesOldAccessTokenOnSentFailure(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-empty-access", now-7200, 3600, "old-access-token"))

	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		return "", nil, 0, "", sentErr(fmt.Errorf("token endpoint returned a 200 response with an empty access_token"))
	}

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, fixedClock(now)); err == nil {
		t.Fatal("expected error")
	}

	persisted := readPersisted(t, credPath)
	if persisted.Issuer["https://issuer.example"].AccessToken != "old-access-token" {
		t.Fatalf("expected old working credential preserved, got %+v", persisted.Issuer["https://issuer.example"])
	}
}

// ---------------------------------------------------------------------------
// C-9 — redirect ban and timeout (AB-10 redirect arm)
// ---------------------------------------------------------------------------

func TestRefreshAccessTokenRefusesRedirectFromTokenEndpoint(t *testing.T) {
	var secondCalled int32
	second := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&secondCalled, 1)
	}))
	defer second.Close()

	mux := http.NewServeMux()
	var first *httptest.Server
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"token_endpoint": %q}`, first.URL+"/token")
	})
	mux.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, second.URL, http.StatusTemporaryRedirect)
	})
	first = httptest.NewServer(mux)
	defer first.Close()

	_, _, _, _, err := refreshAccessToken(nil, "client", first.URL, "refresh-token", time.Second)
	if err == nil {
		t.Fatal("expected error when the token endpoint answers with a redirect")
	}
	if atomic.LoadInt32(&secondCalled) != 0 {
		t.Fatal("the redirect target must never receive the credential-bearing request")
	}
}

// TestRefreshAccessTokenBoundsAHungDiscoveryEndpoint proves C-9's timeout is
// actually wired into the request, not merely claimed: a discovery handler
// that never responds inside the configured timeout must still return in
// bounded time. This matters beyond the request itself — C-1's mutex is
// held across the whole exchange, so an unbounded discovery request would
// stall every authenticated call in the process, not just this one
// (VPL-189-CL21).
func TestRefreshAccessTokenBoundsAHungDiscoveryEndpoint(t *testing.T) {
	unblock := make(chan struct{})

	mux := http.NewServeMux()
	mux.HandleFunc("/.well-known/openid-configuration", func(w http.ResponseWriter, r *http.Request) {
		<-unblock // never returns before the test's timeout budget
	})
	server := httptest.NewServer(mux)
	// Deliberate order: httptest.Server.Close waits for every in-flight
	// handler to return, and the handler above only returns once unblock is
	// closed. Deferred calls run LIFO, so registering close(unblock) after
	// server.Close() makes the handler exit first when this function
	// returns — the reverse order deadlocks Close() waiting on a handler
	// that will never see unblock close until Close() itself has returned.
	defer server.Close()
	defer close(unblock)

	const timeout = 200 * time.Millisecond
	start := time.Now()
	_, _, _, _, err := refreshAccessToken(nil, "client", server.URL, "refresh-token", timeout)
	elapsed := time.Since(start)

	if err == nil {
		t.Fatal("expected an error from a discovery endpoint that never responds")
	}
	var failure *refreshFailureError
	if !errors.As(err, &failure) || failure.kind != refreshFailureNotSent {
		t.Fatalf("expected NotSent classification for a timeout before the token was on the wire, got: %v", err)
	}
	// Generous upper bound (10x the configured timeout) to absorb scheduler
	// jitter in CI without accepting a client.Timeout that silently no-ops.
	if elapsed > 10*timeout {
		t.Fatalf("expected the request to be bounded by %v, took %v", timeout, elapsed)
	}
}

// ---------------------------------------------------------------------------
// C-10 — refuse an unusable token lifetime (AB-11)
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtStopsRotatingAZeroLifetimeToken(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-zero-lifetime", now-7200, 3600, "old-access-token"))

	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		zero := int64(0)
		return "new-access-token", &zero, time.Now().Unix(), "", nil
	}

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, fixedClock(now)); err == nil {
		t.Fatal("expected error for a zero-lifetime token")
	}

	persisted := readPersisted(t, credPath)
	if persisted.Issuer["https://issuer.example"].AccessToken != "old-access-token" {
		t.Fatalf("expected old credential unchanged, got %+v", persisted.Issuer["https://issuer.example"])
	}
}

func TestCommitRefreshedCredentialsDefaultsAbsentExpiresInTo3600(t *testing.T) {
	tempDir := t.TempDir()
	path := filepath.Join(tempDir, "credentials.json")
	creds := credentialsFixture("https://issuer.example", "old-refresh", 0, 3600, "old-access-token")

	if err := commitRefreshedCredentials(&creds, "https://issuer.example", path, "new-access-token", nil, time.Now().Unix(), ""); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got := creds.Issuer["https://issuer.example"].ExpiresIn; got != 3600 {
		t.Fatalf("expected default expires_in 3600 for an absent field, got %d", got)
	}
}

// ---------------------------------------------------------------------------
// C-11 — validate the credential record at first contact (AB-12)
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtRejectsNegativeExpiresIn(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", "refresh-tok-negative-expires", now, -1, "old-access-token"))

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(now)); err == nil {
		t.Fatal("expected error for a negative expires_in")
	}
}

func TestClientAuthHeaderAtRejectsNonNumericIssuedAt(t *testing.T) {
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	raw := `{"issuer":{"https://issuer.example":{"access_token":"a","client_id":"c","expires_in":3600,"issued_at":"corrupt","refresh_token":"r","scopes":[]}},"registry":{"https://registry.example.com":"https://issuer.example"}}`
	if err := os.WriteFile(credPath, []byte(raw), 0o644); err != nil {
		t.Fatal(err)
	}

	if _, err := clientAuthHeaderAt(credPath, "https://registry.example.com", noRefresherExpected(t), fixedClock(time.Now().Unix())); err == nil {
		t.Fatal("expected error for a non-numeric issued_at")
	}
}

// ---------------------------------------------------------------------------
// C-14 — no token material in errors
// ---------------------------------------------------------------------------

func TestClientAuthHeaderAtErrorsNeverContainRefreshToken(t *testing.T) {
	resetSpentTokenMemo(t)
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")
	now := time.Now().Unix()
	const secretToken = "super-secret-refresh-token-value"
	writeFixture(t, credPath, credentialsFixture("https://issuer.example", secretToken, now-7200, 3600, "old-access-token"))

	refresher := func(_ *string, _, _, _ string) (string, *int64, int64, string, error) {
		return "", nil, 0, "", sentErr(fmt.Errorf("token endpoint rejected the request"))
	}

	_, err := clientAuthHeaderAt(credPath, "https://registry.example.com", refresher, fixedClock(now))
	if err == nil {
		t.Fatal("expected error")
	}
	if strings.Contains(err.Error(), secretToken) {
		t.Fatalf("error message must not contain the refresh token value: %v", err)
	}
}
