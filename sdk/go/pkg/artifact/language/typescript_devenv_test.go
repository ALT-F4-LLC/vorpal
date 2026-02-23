package language

import (
	"testing"
)

// TypeScriptDevelopmentEnvironment tests

func TestNewTypeScriptDevelopmentEnvironment(t *testing.T) {
	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems)

	if builder == nil {
		t.Fatal("expected non-nil builder")
	}

	if builder.name != "test-shell" {
		t.Errorf("expected name %q, got %q", "test-shell", builder.name)
	}

	if len(builder.systems) != len(testSystems) {
		t.Errorf("expected %d systems, got %d", len(testSystems), len(builder.systems))
	}

	if len(builder.artifacts) != 0 {
		t.Errorf("expected empty artifacts, got %d", len(builder.artifacts))
	}

	if len(builder.environments) != 0 {
		t.Errorf("expected empty environments, got %d", len(builder.environments))
	}

	if builder.nodeModules == nil {
		t.Error("expected nodeModules to be initialized (non-nil)")
	}

	if len(builder.nodeModules) != 0 {
		t.Errorf("expected empty nodeModules, got %d", len(builder.nodeModules))
	}

	if len(builder.secrets) != 0 {
		t.Errorf("expected empty secrets, got %d", len(builder.secrets))
	}
}

func TestTypeScriptDevelopmentEnvironment_WithArtifacts(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithArtifacts(artifacts)

	if result != builder {
		t.Error("WithArtifacts should return the same builder for chaining")
	}

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact, got %d", len(builder.artifacts))
	}
}

func TestTypeScriptDevelopmentEnvironment_WithEnvironments(t *testing.T) {
	envs := []string{"FOO=bar"}

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithEnvironments(envs)

	if result != builder {
		t.Error("WithEnvironments should return the same builder for chaining")
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment, got %d", len(builder.environments))
	}
}

func TestTypeScriptDevelopmentEnvironment_WithSecrets(t *testing.T) {
	secrets := map[string]string{"key": "value"}

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithSecrets(secrets)

	if result != builder {
		t.Error("WithSecrets should return the same builder for chaining")
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret, got %d", len(builder.secrets))
	}
}

func TestTypeScriptDevelopmentEnvironment_WithNodeModule(t *testing.T) {
	digest := "abc123"

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithNodeModule("@vorpal/sdk", &digest)

	if result != builder {
		t.Error("WithNodeModule should return the same builder for chaining")
	}

	if len(builder.nodeModules) != 1 {
		t.Errorf("expected 1 node module, got %d", len(builder.nodeModules))
	}

	if builder.nodeModules["@vorpal/sdk"] == nil {
		t.Error("expected node module '@vorpal/sdk' to be set")
	}

	if *builder.nodeModules["@vorpal/sdk"] != "abc123" {
		t.Errorf("expected digest %q, got %q", "abc123", *builder.nodeModules["@vorpal/sdk"])
	}
}

func TestTypeScriptDevelopmentEnvironment_WithNodeModule_Multiple(t *testing.T) {
	d1 := "digest-aaa"
	d2 := "digest-bbb"

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems).
		WithNodeModule("@vorpal/sdk", &d1).
		WithNodeModule("lodash", &d2)

	if len(builder.nodeModules) != 2 {
		t.Errorf("expected 2 node modules, got %d", len(builder.nodeModules))
	}

	if *builder.nodeModules["@vorpal/sdk"] != "digest-aaa" {
		t.Errorf("expected digest %q for @vorpal/sdk, got %q", "digest-aaa", *builder.nodeModules["@vorpal/sdk"])
	}

	if *builder.nodeModules["lodash"] != "digest-bbb" {
		t.Errorf("expected digest %q for lodash, got %q", "digest-bbb", *builder.nodeModules["lodash"])
	}
}

func TestTypeScriptDevelopmentEnvironment_WithNodeModule_Overwrite(t *testing.T) {
	d1 := "digest-old"
	d2 := "digest-new"

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems).
		WithNodeModule("@vorpal/sdk", &d1).
		WithNodeModule("@vorpal/sdk", &d2)

	if len(builder.nodeModules) != 1 {
		t.Errorf("expected 1 node module after overwrite, got %d", len(builder.nodeModules))
	}

	if *builder.nodeModules["@vorpal/sdk"] != "digest-new" {
		t.Errorf("expected digest %q after overwrite, got %q", "digest-new", *builder.nodeModules["@vorpal/sdk"])
	}
}

func TestTypeScriptDevelopmentEnvironment_Chaining(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}
	nodeDigest := "node-digest-1"

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems).
		WithArtifacts(artifacts).
		WithEnvironments([]string{"FOO=bar"}).
		WithNodeModule("@vorpal/sdk", &nodeDigest).
		WithSecrets(map[string]string{"key": "value"})

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact after chaining, got %d", len(builder.artifacts))
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment after chaining, got %d", len(builder.environments))
	}

	if len(builder.nodeModules) != 1 {
		t.Errorf("expected 1 node module after chaining, got %d", len(builder.nodeModules))
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret after chaining, got %d", len(builder.secrets))
	}
}
