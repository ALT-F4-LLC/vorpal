package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// Original function to restore after tests
var originalGetKeyCredentialsPathFunc = getKeyCredentialsPathFunc

// Mock function for GetKeyCredentialsPath
func mockGetKeyCredentialsPath(path string) func() string {
	return func() string {
		return path
	}
}

func TestClientAuthHeaderNoFile(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test with non-existent file (should return empty string, no error)
	header, err := ClientAuthHeader("https://registry.example.com")
	if err != nil {
		t.Fatalf("expected no error when credentials file doesn't exist, got: %v", err)
	}
	if header != "" {
		t.Fatalf("expected empty header when credentials file doesn't exist, got: %q", header)
	}
}

func TestClientAuthHeaderValid(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Create valid credentials file with IssuedAt set to now (token not expired)
	credentials := VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"example-issuer": {
				AccessToken:  "test-access-token-12345",
				ClientId:     "test-client-id",
				ExpiresIn:    3600,
				IssuedAt:     time.Now().Unix(),
				RefreshToken: "test-refresh-token",
				Scopes:       []string{"read", "write"},
			},
		},
		Registry: map[string]string{
			"https://registry.example.com": "example-issuer",
		},
	}

	credData, err := json.Marshal(credentials)
	if err != nil {
		t.Fatalf("failed to marshal credentials: %v", err)
	}

	if err := os.WriteFile(credPath, credData, 0644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test with valid credentials
	header, err := ClientAuthHeader("https://registry.example.com")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	expected := "Bearer test-access-token-12345"
	if header != expected {
		t.Fatalf("expected header %q, got %q", expected, header)
	}
}

func TestClientAuthHeaderRegistryNotFound(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Create credentials file without the requested registry
	credentials := VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"example-issuer": {
				AccessToken:  "test-access-token",
				ClientId:     "test-client-id",
				ExpiresIn:    3600,
				IssuedAt:     time.Now().Unix(),
				RefreshToken: "test-refresh-token",
				Scopes:       []string{"read"},
			},
		},
		Registry: map[string]string{
			"https://other-registry.example.com": "example-issuer",
		},
	}

	credData, err := json.Marshal(credentials)
	if err != nil {
		t.Fatalf("failed to marshal credentials: %v", err)
	}

	if err := os.WriteFile(credPath, credData, 0644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test with registry not in credentials - should return empty string (allows unauthenticated)
	header, err := ClientAuthHeader("https://registry.example.com")
	if err != nil {
		t.Fatalf("expected no error for registry not found, got: %v", err)
	}
	if header != "" {
		t.Fatalf("expected empty header for registry not found, got: %q", header)
	}
}

func TestClientAuthHeaderIssuerNotFound(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Create credentials file with registry pointing to non-existent issuer
	credentials := VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"different-issuer": {
				AccessToken:  "test-access-token",
				ClientId:     "test-client-id",
				ExpiresIn:    3600,
				IssuedAt:     time.Now().Unix(),
				RefreshToken: "test-refresh-token",
				Scopes:       []string{"read"},
			},
		},
		Registry: map[string]string{
			"https://registry.example.com": "missing-issuer",
		},
	}

	credData, err := json.Marshal(credentials)
	if err != nil {
		t.Fatalf("failed to marshal credentials: %v", err)
	}

	if err := os.WriteFile(credPath, credData, 0644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test with issuer not in credentials
	_, err = ClientAuthHeader("https://registry.example.com")
	if err == nil {
		t.Fatal("expected error for issuer not found, got nil")
	}

	expectedError := "no credentials for issuer"
	if err.Error()[:len(expectedError)] != expectedError {
		t.Fatalf("expected error containing %q, got %q", expectedError, err.Error())
	}
}

func TestClientAuthHeaderInvalidJSON(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Create invalid JSON file
	invalidJSON := []byte(`{"invalid": json}`)
	if err := os.WriteFile(credPath, invalidJSON, 0644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test with invalid JSON
	_, err := ClientAuthHeader("https://registry.example.com")
	if err == nil {
		t.Fatal("expected error for invalid JSON, got nil")
	}

	expectedError := "failed to parse credentials"
	if err.Error()[:len(expectedError)] != expectedError {
		t.Fatalf("expected error containing %q, got %q", expectedError, err.Error())
	}
}

func TestClientAuthHeaderMultipleRegistries(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	credPath := filepath.Join(tempDir, "credentials.json")

	// Create credentials file with multiple registries and issuers
	now := time.Now().Unix()
	credentials := VorpalCredentials{
		Issuer: map[string]VorpalCredentialsContent{
			"issuer-one": {
				AccessToken:  "token-one",
				ClientId:     "client-one",
				ExpiresIn:    3600,
				IssuedAt:     now,
				RefreshToken: "refresh-one",
				Scopes:       []string{"read"},
			},
			"issuer-two": {
				AccessToken:  "token-two",
				ClientId:     "client-two",
				ExpiresIn:    7200,
				IssuedAt:     now,
				RefreshToken: "refresh-two",
				Scopes:       []string{"read", "write"},
			},
		},
		Registry: map[string]string{
			"https://registry1.example.com": "issuer-one",
			"https://registry2.example.com": "issuer-two",
		},
	}

	credData, err := json.Marshal(credentials)
	if err != nil {
		t.Fatalf("failed to marshal credentials: %v", err)
	}

	if err := os.WriteFile(credPath, credData, 0644); err != nil {
		t.Fatalf("failed to write credentials file: %v", err)
	}

	// Mock the path function
	getKeyCredentialsPathFunc = mockGetKeyCredentialsPath(credPath)
	defer func() { getKeyCredentialsPathFunc = originalGetKeyCredentialsPathFunc }()

	// Test registry 1
	header1, err := ClientAuthHeader("https://registry1.example.com")
	if err != nil {
		t.Fatalf("unexpected error for registry1: %v", err)
	}
	if header1 != "Bearer token-one" {
		t.Fatalf("expected 'Bearer token-one', got %q", header1)
	}

	// Test registry 2
	header2, err := ClientAuthHeader("https://registry2.example.com")
	if err != nil {
		t.Fatalf("unexpected error for registry2: %v", err)
	}
	if header2 != "Bearer token-two" {
		t.Fatalf("expected 'Bearer token-two', got %q", header2)
	}
}

func TestGetKeyCredentialsPath(t *testing.T) {
	// Test the path helper functions
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
