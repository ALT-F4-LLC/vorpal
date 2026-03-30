package config

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

func strPtr(s string) *string {
	return &s
}

func TestSerializeArtifactJSON_EmptySlicesNotNull(t *testing.T) {
	// When all repeated fields are nil (Go zero value), the serializer
	// must produce [] in JSON, not null, matching Rust serde output.
	a := &artifact.Artifact{
		Target:  artifact.ArtifactSystem_AARCH64_DARWIN,
		Name:    "test",
		Systems: []artifact.ArtifactSystem{artifact.ArtifactSystem_AARCH64_DARWIN},
		Steps: []*artifact.ArtifactStep{
			{
				Script: strPtr("echo hello"),
				// Secrets, Arguments, Artifacts, Environments are nil
			},
		},
		// Sources, Aliases are nil
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(b, &raw); err != nil {
		t.Fatalf("invalid JSON: %v", err)
	}

	// Check top-level slices are arrays, not null
	for _, field := range []string{"sources", "aliases"} {
		val := raw[field]
		if val == nil {
			t.Errorf("field %q is null, expected empty array", field)
			continue
		}
		arr, ok := val.([]interface{})
		if !ok {
			t.Errorf("field %q is not an array: %T", field, val)
			continue
		}
		if len(arr) != 0 {
			t.Errorf("field %q has %d elements, expected 0", field, len(arr))
		}
	}

	// Check step-level slices
	steps := raw["steps"].([]interface{})
	step := steps[0].(map[string]interface{})
	for _, field := range []string{"secrets", "arguments", "artifacts", "environments"} {
		val := step[field]
		if val == nil {
			t.Errorf("step field %q is null, expected empty array", field)
			continue
		}
		arr, ok := val.([]interface{})
		if !ok {
			t.Errorf("step field %q is not an array: %T", field, val)
			continue
		}
		if len(arr) != 0 {
			t.Errorf("step field %q has %d elements, expected 0", field, len(arr))
		}
	}
}

func TestSerializeArtifactJSON_OptionalFieldsNull(t *testing.T) {
	// Optional proto fields (Entrypoint, Script, Digest) must serialize
	// as null when nil, not be omitted.
	a := &artifact.Artifact{
		Target:  artifact.ArtifactSystem_AARCH64_LINUX,
		Name:    "test-optional",
		Systems: []artifact.ArtifactSystem{artifact.ArtifactSystem_AARCH64_LINUX},
		Sources: []*artifact.ArtifactSource{
			{
				Name: "src",
				Path: "/some/path",
				// Digest is nil (optional)
			},
		},
		Steps: []*artifact.ArtifactStep{
			{
				// Entrypoint and Script are nil (optional)
			},
		},
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	jsonStr := string(b)

	// The JSON must contain explicit null for optional fields
	var raw map[string]interface{}
	if err := json.Unmarshal(b, &raw); err != nil {
		t.Fatalf("invalid JSON: %v", err)
	}

	// Source digest must be null
	sources := raw["sources"].([]interface{})
	source := sources[0].(map[string]interface{})
	if _, exists := source["digest"]; !exists {
		t.Errorf("source field 'digest' is missing, expected null. JSON: %s", jsonStr)
	} else if source["digest"] != nil {
		t.Errorf("source field 'digest' is %v, expected null", source["digest"])
	}

	// Step entrypoint and script must be null
	steps := raw["steps"].([]interface{})
	step := steps[0].(map[string]interface{})
	for _, field := range []string{"entrypoint", "script"} {
		if _, exists := step[field]; !exists {
			t.Errorf("step field %q is missing, expected null. JSON: %s", field, jsonStr)
		} else if step[field] != nil {
			t.Errorf("step field %q is %v, expected null", field, step[field])
		}
	}
}

func TestSerializeArtifactJSON_EnumsAsIntegers(t *testing.T) {
	// Enums must serialize as integers, not strings.
	a := &artifact.Artifact{
		Target: artifact.ArtifactSystem_X8664_LINUX, // = 4
		Name:   "test-enums",
		Systems: []artifact.ArtifactSystem{
			artifact.ArtifactSystem_AARCH64_DARWIN, // = 1
			artifact.ArtifactSystem_X8664_LINUX,    // = 4
		},
		Steps: []*artifact.ArtifactStep{
			{Script: strPtr("true")},
		},
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(b, &raw); err != nil {
		t.Fatalf("invalid JSON: %v", err)
	}

	// Target should be 4 (X8664_LINUX)
	target := raw["target"].(float64)
	if target != 4 {
		t.Errorf("target is %v, expected 4", target)
	}

	// Systems should be [1, 4]
	systems := raw["systems"].([]interface{})
	if len(systems) != 2 {
		t.Fatalf("expected 2 systems, got %d", len(systems))
	}
	if systems[0].(float64) != 1 {
		t.Errorf("systems[0] is %v, expected 1", systems[0])
	}
	if systems[1].(float64) != 4 {
		t.Errorf("systems[1] is %v, expected 4", systems[1])
	}
}

func TestSerializeArtifactJSON_FieldOrder(t *testing.T) {
	// Field order must match proto field number order. Go's json.Marshal
	// preserves struct field declaration order, so we verify the JSON
	// key order directly.
	a := &artifact.Artifact{
		Target:  artifact.ArtifactSystem_AARCH64_DARWIN,
		Name:    "test-order",
		Systems: []artifact.ArtifactSystem{artifact.ArtifactSystem_AARCH64_DARWIN},
		Steps: []*artifact.ArtifactStep{
			{Script: strPtr("true")},
		},
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	// Parse as ordered tokens to verify key order
	expectedOrder := []string{"target", "sources", "steps", "systems", "aliases", "name"}
	keys := extractTopLevelKeyOrder(t, b)

	if len(keys) != len(expectedOrder) {
		t.Fatalf("expected %d keys, got %d: %v", len(expectedOrder), len(keys), keys)
	}
	for i, key := range keys {
		if key != expectedOrder[i] {
			t.Errorf("key[%d] = %q, expected %q (full order: %v)", i, key, expectedOrder[i], keys)
		}
	}
}

func TestSerializeArtifactJSON_FullArtifact(t *testing.T) {
	// Round-trip a fully populated artifact and verify all fields are
	// present with correct values.
	digest := "abc123"
	a := &artifact.Artifact{
		Target: artifact.ArtifactSystem_AARCH64_DARWIN,
		Sources: []*artifact.ArtifactSource{
			{
				Digest:   &digest,
				Excludes: []string{"*.tmp"},
				Includes: []string{"src/**"},
				Name:     "main",
				Path:     "https://example.com/source.tar.gz",
			},
		},
		Steps: []*artifact.ArtifactStep{
			{
				Entrypoint: strPtr("/bin/sh"),
				Script:     strPtr("make install"),
				Secrets: []*artifact.ArtifactStepSecret{
					{Name: "API_KEY", Value: "secret123"},
				},
				Arguments:    []string{"--verbose"},
				Artifacts:    []string{"dep1", "dep2"},
				Environments: []string{"PATH=/usr/bin"},
			},
		},
		Systems: []artifact.ArtifactSystem{
			artifact.ArtifactSystem_AARCH64_DARWIN,
			artifact.ArtifactSystem_X8664_LINUX,
		},
		Aliases: []string{"myapp:1.0.0"},
		Name:    "myapp",
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(b, &raw); err != nil {
		t.Fatalf("invalid JSON: %v", err)
	}

	// Verify name
	if raw["name"] != "myapp" {
		t.Errorf("name = %v, expected 'myapp'", raw["name"])
	}

	// Verify target is integer
	if raw["target"].(float64) != 1 {
		t.Errorf("target = %v, expected 1", raw["target"])
	}

	// Verify source digest is string (not null)
	sources := raw["sources"].([]interface{})
	source := sources[0].(map[string]interface{})
	if source["digest"] != "abc123" {
		t.Errorf("source digest = %v, expected 'abc123'", source["digest"])
	}

	// Verify step has all fields
	steps := raw["steps"].([]interface{})
	step := steps[0].(map[string]interface{})
	if step["entrypoint"] != "/bin/sh" {
		t.Errorf("step entrypoint = %v, expected '/bin/sh'", step["entrypoint"])
	}
	if step["script"] != "make install" {
		t.Errorf("step script = %v, expected 'make install'", step["script"])
	}

	secrets := step["secrets"].([]interface{})
	if len(secrets) != 1 {
		t.Fatalf("expected 1 secret, got %d", len(secrets))
	}
	secret := secrets[0].(map[string]interface{})
	if secret["name"] != "API_KEY" {
		t.Errorf("secret name = %v, expected 'API_KEY'", secret["name"])
	}

	// Verify aliases
	aliases := raw["aliases"].([]interface{})
	if len(aliases) != 1 || aliases[0] != "myapp:1.0.0" {
		t.Errorf("aliases = %v, expected ['myapp:1.0.0']", aliases)
	}
}

func TestSerializeArtifactJSON_ZeroValueTarget(t *testing.T) {
	// A zero-value enum (UNKNOWN_SYSTEM = 0) must serialize as 0, not
	// be omitted.
	a := &artifact.Artifact{
		Target:  artifact.ArtifactSystem_UNKNOWN_SYSTEM,
		Name:    "test-zero",
		Systems: []artifact.ArtifactSystem{artifact.ArtifactSystem_UNKNOWN_SYSTEM},
		Steps: []*artifact.ArtifactStep{
			{Script: strPtr("true")},
		},
	}

	b, err := SerializeArtifactJSON(a)
	if err != nil {
		t.Fatalf("SerializeArtifactJSON failed: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(b, &raw); err != nil {
		t.Fatalf("invalid JSON: %v", err)
	}

	if raw["target"].(float64) != 0 {
		t.Errorf("target = %v, expected 0", raw["target"])
	}

	systems := raw["systems"].([]interface{})
	if len(systems) != 1 || systems[0].(float64) != 0 {
		t.Errorf("systems = %v, expected [0]", systems)
	}
}

// extractTopLevelKeyOrder uses json.Decoder to read the top-level object
// keys in declaration order.
func extractTopLevelKeyOrder(t *testing.T, data []byte) []string {
	t.Helper()

	dec := json.NewDecoder(bytes.NewReader(data))

	// Opening brace
	tok, err := dec.Token()
	if err != nil {
		t.Fatalf("failed to read opening token: %v", err)
	}
	if tok != json.Delim('{') {
		t.Fatalf("expected '{', got %v", tok)
	}

	var keys []string
	for dec.More() {
		tok, err := dec.Token()
		if err != nil {
			t.Fatalf("failed to read token: %v", err)
		}
		if key, ok := tok.(string); ok {
			keys = append(keys, key)
			// Skip the value
			skipValue(t, dec)
		}
	}
	return keys
}

// skipValue skips a single JSON value (including nested objects/arrays).
func skipValue(t *testing.T, dec *json.Decoder) {
	t.Helper()

	tok, err := dec.Token()
	if err != nil {
		t.Fatalf("failed to read value token: %v", err)
	}

	switch tok {
	case json.Delim('{'):
		for dec.More() {
			// skip key
			if _, err := dec.Token(); err != nil {
				t.Fatalf("failed to skip key: %v", err)
			}
			skipValue(t, dec)
		}
		// closing brace
		if _, err := dec.Token(); err != nil {
			t.Fatalf("failed to read closing brace: %v", err)
		}
	case json.Delim('['):
		for dec.More() {
			skipValue(t, dec)
		}
		// closing bracket
		if _, err := dec.Token(); err != nil {
			t.Fatalf("failed to read closing bracket: %v", err)
		}
	}
	// Primitive values are already consumed by the initial Token() call.
}
