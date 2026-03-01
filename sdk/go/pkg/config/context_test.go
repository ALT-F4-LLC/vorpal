package config

import (
	"strings"
	"testing"
)

func TestParseArtifactAlias(t *testing.T) {
	tests := []struct {
		name              string
		alias             string
		expectedName      string
		expectedNamespace string
		expectedTag       string
		expectError       bool
		errorContains     string
	}{
		// Basic formats
		{
			name:              "name only",
			alias:             "myapp",
			expectedName:      "myapp",
			expectedNamespace: "library",
			expectedTag:       "latest",
			expectError:       false,
		},
		{
			name:              "name with tag",
			alias:             "myapp:1.0.0",
			expectedName:      "myapp",
			expectedNamespace: "library",
			expectedTag:       "1.0.0",
			expectError:       false,
		},
		{
			name:              "namespace and name",
			alias:             "team/myapp",
			expectedName:      "myapp",
			expectedNamespace: "team",
			expectedTag:       "latest",
			expectError:       false,
		},
		{
			name:              "full format",
			alias:             "team/myapp:v2.1",
			expectedName:      "myapp",
			expectedNamespace: "team",
			expectedTag:       "v2.1",
			expectError:       false,
		},

		// Real-world examples from codebase
		{
			name:              "linux-vorpal:latest",
			alias:             "linux-vorpal:latest",
			expectedName:      "linux-vorpal",
			expectedNamespace: "library",
			expectedTag:       "latest",
			expectError:       false,
		},
		{
			name:              "gh:2.87.3",
			alias:             "gh:2.87.3",
			expectedName:      "gh",
			expectedNamespace: "library",
			expectedTag:       "2.87.3",
			expectError:       false,
		},
		{
			name:              "protoc:34.0",
			alias:             "protoc:34.0",
			expectedName:      "protoc",
			expectedNamespace: "library",
			expectedTag:       "34.0",
			expectError:       false,
		},
		{
			name:              "protoc-gen-go:1.36.11",
			alias:             "protoc-gen-go:1.36.11",
			expectedName:      "protoc-gen-go",
			expectedNamespace: "library",
			expectedTag:       "1.36.11",
			expectError:       false,
		},

		// Edge cases - multiple colons (rightmost is split point, but colon in name is invalid)
		{
			name:          "name with multiple colons",
			alias:         "name:tag:extra",
			expectError:   true,
			errorContains: "name contains invalid characters",
		},

		// Names with valid special characters
		{
			name:              "name with hyphens",
			alias:             "my-app-name:v1.0",
			expectedName:      "my-app-name",
			expectedNamespace: "library",
			expectedTag:       "v1.0",
			expectError:       false,
		},
		{
			name:              "name with underscores",
			alias:             "my_app_name:v1.0",
			expectedName:      "my_app_name",
			expectedNamespace: "library",
			expectedTag:       "v1.0",
			expectError:       false,
		},
		{
			name:              "namespace with hyphens",
			alias:             "my-team/my-app:v1.0",
			expectedName:      "my-app",
			expectedNamespace: "my-team",
			expectedTag:       "v1.0",
			expectError:       false,
		},

		// Semantic versions
		{
			name:              "semantic version tag",
			alias:             "myapp:1.2.3",
			expectedName:      "myapp",
			expectedNamespace: "library",
			expectedTag:       "1.2.3",
			expectError:       false,
		},
		{
			name:              "semantic version with v prefix",
			alias:             "myapp:v1.2.3",
			expectedName:      "myapp",
			expectedNamespace: "library",
			expectedTag:       "v1.2.3",
			expectError:       false,
		},

		// Numeric components
		{
			name:              "numeric name",
			alias:             "123:latest",
			expectedName:      "123",
			expectedNamespace: "library",
			expectedTag:       "latest",
			expectError:       false,
		},
		{
			name:              "numeric namespace",
			alias:             "123/myapp:v1.0",
			expectedName:      "myapp",
			expectedNamespace: "123",
			expectedTag:       "v1.0",
			expectError:       false,
		},

		// Error cases
		{
			name:          "empty string",
			alias:         "",
			expectError:   true,
			errorContains: "alias cannot be empty",
		},
		{
			name:          "empty tag",
			alias:         "name:",
			expectError:   true,
			errorContains: "tag cannot be empty",
		},
		{
			name:          "too many slashes",
			alias:         "a/b/c",
			expectError:   true,
			errorContains: "too many path separators",
		},
		{
			name:          "empty namespace before slash",
			alias:         "/name",
			expectError:   true,
			errorContains: "namespace cannot be empty",
		},
		{
			name:          "empty name after slash",
			alias:         "namespace/",
			expectError:   true,
			errorContains: "name is required",
		},
		{
			name:          "too long alias",
			alias:         strings.Repeat("a", 256),
			expectError:   true,
			errorContains: "alias too long",
		},
		{
			name:          "only slash",
			alias:         "/",
			expectError:   true,
			errorContains: "namespace cannot be empty",
		},
		{
			name:          "only colon",
			alias:         ":",
			expectError:   true,
			errorContains: "tag cannot be empty",
		},

		// Invalid character cases
		{
			name:          "name with space",
			alias:         "my app",
			expectError:   true,
			errorContains: "name contains invalid characters",
		},
		{
			name:          "name with @",
			alias:         "my@app",
			expectError:   true,
			errorContains: "name contains invalid characters",
		},
		{
			name:          "name with #",
			alias:         "my#app",
			expectError:   true,
			errorContains: "name contains invalid characters",
		},
		{
			name:          "namespace with space",
			alias:         "my team/myapp",
			expectError:   true,
			errorContains: "namespace contains invalid characters",
		},
		{
			name:          "namespace with @",
			alias:         "te@m/myapp",
			expectError:   true,
			errorContains: "namespace contains invalid characters",
		},
		{
			name:          "tag with space",
			alias:         "myapp:v 1.0",
			expectError:   true,
			errorContains: "tag contains invalid characters",
		},
		{
			name:          "tag with #",
			alias:         "myapp:v1#beta",
			expectError:   true,
			errorContains: "tag contains invalid characters",
		},
		{
			name:          "name with exclamation mark",
			alias:         "myapp!:v1.0",
			expectError:   true,
			errorContains: "name contains invalid characters",
		},

		// Plus signs are valid
		{
			name:              "name with plus sign",
			alias:             "c++:latest",
			expectedName:      "c++",
			expectedNamespace: "library",
			expectedTag:       "latest",
			expectError:       false,
		},
		{
			name:              "tag with plus sign",
			alias:             "myapp:v1.0+build.1",
			expectedName:      "myapp",
			expectedNamespace: "library",
			expectedTag:       "v1.0+build.1",
			expectError:       false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parseArtifactAlias(tt.alias)

			if tt.expectError {
				if err == nil {
					t.Errorf("expected error but got none")
					return
				}
				if tt.errorContains != "" && !strings.Contains(err.Error(), tt.errorContains) {
					t.Errorf("expected error to contain %q, got %q", tt.errorContains, err.Error())
				}
				return
			}

			if err != nil {
				t.Errorf("unexpected error: %v", err)
				return
			}

			if result == nil {
				t.Errorf("expected result but got nil")
				return
			}

			if result.Name != tt.expectedName {
				t.Errorf("expected name %q, got %q", tt.expectedName, result.Name)
			}

			if result.Namespace != tt.expectedNamespace {
				t.Errorf("expected namespace %q, got %q", tt.expectedNamespace, result.Namespace)
			}

			if result.Tag != tt.expectedTag {
				t.Errorf("expected tag %q, got %q", tt.expectedTag, result.Tag)
			}
		})
	}
}

// TestParseArtifactAliasDefaults specifically tests default value application
func TestParseArtifactAliasDefaults(t *testing.T) {
	tests := []struct {
		name             string
		alias            string
		expectDefaultTag bool
		expectDefaultNS  bool
	}{
		{
			name:             "both defaults applied",
			alias:            "myapp",
			expectDefaultTag: true,
			expectDefaultNS:  true,
		},
		{
			name:             "only tag default applied",
			alias:            "team/myapp",
			expectDefaultTag: true,
			expectDefaultNS:  false,
		},
		{
			name:             "only namespace default applied",
			alias:            "myapp:v1.0",
			expectDefaultTag: false,
			expectDefaultNS:  true,
		},
		{
			name:             "no defaults applied",
			alias:            "team/myapp:v1.0",
			expectDefaultTag: false,
			expectDefaultNS:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parseArtifactAlias(tt.alias)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if tt.expectDefaultTag && result.Tag != "latest" {
				t.Errorf("expected default tag 'latest', got %q", result.Tag)
			}

			if tt.expectDefaultNS && result.Namespace != "library" {
				t.Errorf("expected default namespace 'library', got %q", result.Namespace)
			}
		})
	}
}
