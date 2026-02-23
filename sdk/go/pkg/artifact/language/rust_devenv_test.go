package language

import (
	"testing"
)

// RustDevelopmentEnvironment tests

func TestNewRustDevelopmentEnvironment(t *testing.T) {
	builder := NewRustDevelopmentEnvironment("test-shell", testSystems)

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

	if !builder.includeProtoc {
		t.Error("expected includeProtoc to be true by default")
	}
}

func TestRustDevelopmentEnvironment_WithArtifacts(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}

	builder := NewRustDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithArtifacts(artifacts)

	if result != builder {
		t.Error("WithArtifacts should return the same builder for chaining")
	}

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact, got %d", len(builder.artifacts))
	}
}

func TestRustDevelopmentEnvironment_WithEnvironments(t *testing.T) {
	envs := []string{"FOO=bar"}

	builder := NewRustDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithEnvironments(envs)

	if result != builder {
		t.Error("WithEnvironments should return the same builder for chaining")
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment, got %d", len(builder.environments))
	}
}

func TestRustDevelopmentEnvironment_WithSecrets(t *testing.T) {
	secrets := map[string]string{"key": "value"}

	builder := NewRustDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithSecrets(secrets)

	if result != builder {
		t.Error("WithSecrets should return the same builder for chaining")
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret, got %d", len(builder.secrets))
	}
}

func TestRustDevelopmentEnvironment_WithoutProtoc(t *testing.T) {
	builder := NewRustDevelopmentEnvironment("test-shell", testSystems)

	if !builder.includeProtoc {
		t.Fatal("includeProtoc should be true before WithoutProtoc")
	}

	result := builder.WithoutProtoc()

	if result != builder {
		t.Error("WithoutProtoc should return the same builder for chaining")
	}

	if builder.includeProtoc {
		t.Error("expected includeProtoc to be false after WithoutProtoc")
	}
}

func TestRustDevelopmentEnvironment_Chaining(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}

	builder := NewRustDevelopmentEnvironment("test-shell", testSystems).
		WithArtifacts(artifacts).
		WithEnvironments([]string{"FOO=bar"}).
		WithSecrets(map[string]string{"key": "value"}).
		WithoutProtoc()

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact after chaining, got %d", len(builder.artifacts))
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment after chaining, got %d", len(builder.environments))
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret after chaining, got %d", len(builder.secrets))
	}

	if builder.includeProtoc {
		t.Error("expected includeProtoc to be false after chained WithoutProtoc")
	}
}
