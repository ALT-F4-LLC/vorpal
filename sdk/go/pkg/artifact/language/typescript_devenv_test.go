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

func TestTypeScriptDevelopmentEnvironment_Chaining(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}

	builder := NewTypeScriptDevelopmentEnvironment("test-shell", testSystems).
		WithArtifacts(artifacts).
		WithEnvironments([]string{"FOO=bar"}).
		WithSecrets(map[string]string{"key": "value"})

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact after chaining, got %d", len(builder.artifacts))
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment after chaining, got %d", len(builder.environments))
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret after chaining, got %d", len(builder.secrets))
	}
}
