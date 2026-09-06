package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
	"syscall"
	"testing"
	"time"
)

func twoIssuerFixture() VorpalCredentials {
	return VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"issuer-a": {
				AccessToken:  "a-old-access",
				ClientId:     "client-a",
				ExpiresIn:    3600,
				IssuedAt:     1_700_000_000,
				RefreshToken: "a-old-refresh",
				Scopes:       []string{"openid"},
			},
			"issuer-b": {
				AccessToken:  "b-old-access",
				ClientId:     "client-b",
				ExpiresIn:    3600,
				IssuedAt:     1_700_000_000,
				RefreshToken: "b-old-refresh",
				Scopes:       []string{"openid"},
			},
		},
		Registry: map[string]string{},
	}
}

func seedTwoIssuerFixture(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "credentials.json")
	data, err := json.MarshalIndent(twoIssuerFixture(), "", "  ")
	if err != nil {
		t.Fatalf("failed to marshal fixture: %v", err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatalf("failed to seed fixture: %v", err)
	}
	return path
}

// TestCommitRefreshedCredentialsSurvivesTwoConcurrentRefreshesForDifferentIssuers
// is this issue's AC3 test on the Go side, mirroring Rust's
// commit_refreshed_credentials_survives_two_concurrent_refreshes_for_different_issuers
// (sdk/rust/src/context.rs:3721). commitRefreshedCredentials is called
// directly, on two goroutines that each pre-read the file and then rendezvous
// before either commits — that path never touches credentialsRefreshState.mu
// (the process mutex lives only in clientAuthHeaderAt), so the only thing
// that can serialize these two commits is the cross-process file lock. A test
// built as "two clientAuthHeaderAt calls" would be serialized for free by the
// process mutex and pass against an unlocked commit path.
//
// flock(2) contends per open file description rather than per process, so two
// goroutines opening the sidecar separately exclude each other exactly as two
// processes would. What this shape cannot establish is the cross-SDK pairing
// AC3 also asks for; see the change summary's gap for that.
func TestCommitRefreshedCredentialsSurvivesTwoConcurrentRefreshesForDifferentIssuers(t *testing.T) {
	path := seedTwoIssuerFixture(t)

	var ready sync.WaitGroup
	var release sync.WaitGroup
	var done sync.WaitGroup
	ready.Add(2)
	release.Add(1)
	done.Add(2)

	errs := make([]error, 2)
	commit := func(index int, issuer, accessToken, refreshToken string) {
		defer done.Done()
		// Read the whole document before either writer commits: this is the
		// pre-exchange snapshot every caller of this function holds, and the
		// state a lost update discards.
		if _, err := os.ReadFile(path); err != nil {
			errs[index] = err
			ready.Done()
			return
		}
		ready.Done()
		release.Wait()

		expires := int64(3600)
		errs[index] = commitRefreshedCredentials(
			issuer,
			path,
			accessToken,
			&expires,
			1_700_000_500,
			refreshToken,
		)
	}

	go commit(0, "issuer-a", "a-new-access", "a-new-refresh")
	go commit(1, "issuer-b", "b-new-access", "b-new-refresh")

	ready.Wait()
	release.Done()
	done.Wait()

	for index, err := range errs {
		if err != nil {
			t.Fatalf("commit %d failed: %v", index, err)
		}
	}

	final := readPersisted(t, path)

	a, ok := final.Issuer["issuer-a"]
	if !ok {
		t.Fatal("issuer-a's entry must survive the concurrent refresh")
	}
	if a.AccessToken != "a-new-access" || a.RefreshToken != "a-new-refresh" {
		t.Fatalf("issuer-a lost its update: access=%q refresh=%q", a.AccessToken, a.RefreshToken)
	}

	b, ok := final.Issuer["issuer-b"]
	if !ok {
		t.Fatal("issuer-b's entry must survive the concurrent refresh")
	}
	if b.AccessToken != "b-new-access" || b.RefreshToken != "b-new-refresh" {
		t.Fatalf("issuer-b lost its update: access=%q refresh=%q", b.AccessToken, b.RefreshToken)
	}
}

// TestNaiveReadModifyWriteWithoutTheLockLosesAConcurrentIssuersUpdate is the
// mandatory negative control for the test above: it reproduces the defect in
// the exact shape this issue describes — read the whole file, mutate one
// issuer's entry, write the whole file back, no lock at all — so the passing
// test above is known to be capable of failing. Without this, a harness whose
// two writers never actually overlap would pass having tested nothing.
// Mirrors Rust's naive_read_modify_write_without_the_lock_loses_a_concurrent_issuers_update
// (sdk/rust/src/context.rs:3841).
func TestNaiveReadModifyWriteWithoutTheLockLosesAConcurrentIssuersUpdate(t *testing.T) {
	path := seedTwoIssuerFixture(t)

	var ready sync.WaitGroup
	var release sync.WaitGroup
	var done sync.WaitGroup
	ready.Add(2)
	release.Add(1)
	done.Add(2)

	naiveCommit := func(issuer, accessToken string) {
		defer done.Done()

		data, err := os.ReadFile(path)
		if err != nil {
			t.Errorf("read credentials: %v", err)
			ready.Done()
			return
		}
		var credentials VorpalCredentials
		if err := json.Unmarshal(data, &credentials); err != nil {
			t.Errorf("parse credentials: %v", err)
			ready.Done()
			return
		}

		// Both writers must have read the pre-image before either commits,
		// or this would prove only ordinary sequential safety.
		ready.Done()
		release.Wait()

		issuerCreds := credentials.Issuer[issuer]
		issuerCreds.AccessToken = accessToken
		credentials.Issuer[issuer] = issuerCreds

		merged, err := json.MarshalIndent(credentials, "", "  ")
		if err != nil {
			t.Errorf("serialize credentials: %v", err)
			return
		}
		if err := writeCredentialsSecure(path, merged); err != nil {
			t.Errorf("write credentials: %v", err)
		}
	}

	go naiveCommit("issuer-a", "a-new-access")
	go naiveCommit("issuer-b", "b-new-access")

	ready.Wait()
	release.Done()
	done.Wait()

	final := readPersisted(t, path)
	aSurvived := final.Issuer["issuer-a"].AccessToken == "a-new-access"
	bSurvived := final.Issuer["issuer-b"].AccessToken == "b-new-access"

	if aSurvived && bSurvived {
		t.Fatal("an unlocked read-modify-write was expected to lose one issuer's update, but both survived")
	}
}

// TestAcquireCredentialsLockExcludesASecondAcquirerUntilTheFirstReleases is
// the positive control on the lock primitive itself: without it, a helper
// that opened the sidecar but never actually took the kernel lock would still
// let every other test here pass on timing alone.
func TestAcquireCredentialsLockExcludesASecondAcquirerUntilTheFirstReleases(t *testing.T) {
	path := filepath.Join(t.TempDir(), "credentials.json")

	releaseFirst, err := acquireCredentialsLock(path)
	if err != nil {
		t.Fatalf("acquire first lock: %v", err)
	}

	type acquisition struct {
		elapsed time.Duration
		err     error
		release func()
	}
	second := make(chan acquisition, 1)

	go func() {
		start := time.Now()
		release, err := acquireCredentialsLock(path)
		second <- acquisition{elapsed: time.Since(start), err: err, release: release}
	}()

	time.Sleep(200 * time.Millisecond)
	releaseFirst()

	got := <-second
	if got.err != nil {
		t.Fatalf("acquire second lock: %v", got.err)
	}
	got.release()

	if got.elapsed < 150*time.Millisecond {
		t.Fatalf("second acquirer got the lock after %v, so it did not wait for the first to release", got.elapsed)
	}
}

// TestFlockOnARenamedPathDoesNotExcludeAPostRenameOpener is the negative
// control against locking the credentials file itself, exercised on the raw
// flock(2) primitive rather than this package's helper. Writer A locks the
// original file at path; a commit then renames a replacement onto that same
// path, which is what writeCredentialsSecure does every time. Writer B,
// opening path fresh, acquires immediately — the lock followed the old inode,
// not the name. This is why credentialsLockPath must name a sidecar, and it
// makes a future refactor toward locking credentials.json fail loudly.
// Mirrors Rust's flock_on_a_renamed_path_does_not_exclude_a_post_rename_opener
// (sdk/rust/src/context.rs:3988).
func TestFlockOnARenamedPathDoesNotExcludeAPostRenameOpener(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "credentials.json")
	if err := os.WriteFile(path, []byte("original"), 0o600); err != nil {
		t.Fatalf("seed original file: %v", err)
	}

	fileA, err := os.OpenFile(path, os.O_RDWR, 0o600)
	if err != nil {
		t.Fatalf("open original: %v", err)
	}
	defer fileA.Close()

	if err := syscall.Flock(int(fileA.Fd()), syscall.LOCK_EX); err != nil {
		t.Fatalf("lock original: %v", err)
	}

	replacement := filepath.Join(dir, "replacement")
	if err := os.WriteFile(replacement, []byte("replacement"), 0o600); err != nil {
		t.Fatalf("seed replacement: %v", err)
	}
	if err := os.Rename(replacement, path); err != nil {
		t.Fatalf("rename replacement onto path: %v", err)
	}

	fileB, err := os.OpenFile(path, os.O_RDWR, 0o600)
	if err != nil {
		t.Fatalf("open post-rename path: %v", err)
	}
	defer fileB.Close()

	if err := syscall.Flock(int(fileB.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		t.Fatalf("a lock on the credentials path was expected not to exclude a post-rename opener, but the second acquire failed: %v", err)
	}
}

// TestAcquireCredentialsLockRefusesASymlinkedSidecar pins the O_NOFOLLOW
// refusal: a symlink planted at the lock path must not be followed, so a
// principal who can create entries in the key directory cannot steer the SDK
// into creating or truncating a file of their choosing. The assertion is on
// the target file, not the error text.
func TestAcquireCredentialsLockRefusesASymlinkedSidecar(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "credentials.json")
	sentinel := filepath.Join(dir, "sentinel")

	if err := os.Symlink(sentinel, credentialsLockPath(path)); err != nil {
		t.Fatalf("plant symlink at the lock path: %v", err)
	}

	release, err := acquireCredentialsLock(path)
	if err == nil {
		release()
		t.Fatal("acquiring the lock through a symlinked sidecar must fail")
	}

	if _, err := os.Lstat(sentinel); !os.IsNotExist(err) {
		t.Fatalf("the symlink target must not be created or modified, got err=%v", err)
	}
}

// TestCommitRefreshedCredentialsPreservesAnIssuerAddedAfterTheCallersRead
// pins the merge direction: the document written back is the one re-read
// under the lock, never the caller's pre-exchange snapshot, so an issuer a
// concurrent writer added while the exchange was in flight survives.
func TestCommitRefreshedCredentialsPreservesAnIssuerAddedAfterTheCallersRead(t *testing.T) {
	path := seedTwoIssuerFixture(t)

	added := twoIssuerFixture()
	added.Issuer["issuer-c"] = VorpalCredentialsContent{
		AccessToken:  "c-access",
		ClientId:     "client-c",
		ExpiresIn:    3600,
		IssuedAt:     1_700_000_000,
		RefreshToken: "c-refresh",
		Scopes:       []string{"openid"},
	}
	data, err := json.MarshalIndent(added, "", "  ")
	if err != nil {
		t.Fatalf("marshal concurrent write: %v", err)
	}
	if err := writeCredentialsSecure(path, data); err != nil {
		t.Fatalf("apply concurrent write: %v", err)
	}

	expires := int64(3600)
	if err := commitRefreshedCredentials("issuer-a", path, "a-new-access", &expires, 1_700_000_500, "a-new-refresh"); err != nil {
		t.Fatalf("commit: %v", err)
	}

	final := readPersisted(t, path)
	if got := final.Issuer["issuer-a"].AccessToken; got != "a-new-access" {
		t.Fatalf("issuer-a was not updated: %q", got)
	}
	if _, ok := final.Issuer["issuer-c"]; !ok {
		t.Fatal("an issuer added after the caller's read must survive the commit")
	}
}

// TestCommitRefreshedCredentialsRefusesAnIssuerRemovedAfterTheCallersRead is
// the second-order case of the merge direction: an issuer another writer
// deleted while the exchange was in flight must not be resurrected by writing
// the caller's snapshot back.
func TestCommitRefreshedCredentialsRefusesAnIssuerRemovedAfterTheCallersRead(t *testing.T) {
	path := seedTwoIssuerFixture(t)

	remaining := twoIssuerFixture()
	delete(remaining.Issuer, "issuer-a")
	data, err := json.MarshalIndent(remaining, "", "  ")
	if err != nil {
		t.Fatalf("marshal concurrent write: %v", err)
	}
	if err := writeCredentialsSecure(path, data); err != nil {
		t.Fatalf("apply concurrent write: %v", err)
	}

	expires := int64(3600)
	err = commitRefreshedCredentials("issuer-a", path, "a-new-access", &expires, 1_700_000_500, "a-new-refresh")
	if err == nil {
		t.Fatal("committing a refresh for an issuer removed after the caller's read must fail")
	}

	if _, ok := readPersisted(t, path).Issuer["issuer-a"]; ok {
		t.Fatal("a deleted issuer must not be resurrected by the commit")
	}
}
