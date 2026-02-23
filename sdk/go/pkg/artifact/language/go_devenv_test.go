package language

import (
	"testing"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

var testSystems = []api.ArtifactSystem{
	api.ArtifactSystem_AARCH64_DARWIN,
	api.ArtifactSystem_AARCH64_LINUX,
	api.ArtifactSystem_X8664_DARWIN,
	api.ArtifactSystem_X8664_LINUX,
}

// GoDevelopmentEnvironment tests

func TestNewGoDevelopmentEnvironment(t *testing.T) {
	builder := NewGoDevelopmentEnvironment("test-shell", testSystems)

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

	if !builder.includeProtocGenGo {
		t.Error("expected includeProtocGenGo to be true by default")
	}

	if !builder.includeProtocGenGoGRPC {
		t.Error("expected includeProtocGenGoGRPC to be true by default")
	}
}

func TestGoDevelopmentEnvironment_WithArtifacts(t *testing.T) {
	a := "digest-1"
	b := "digest-2"
	artifacts := []*string{&a, &b}

	builder := NewGoDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithArtifacts(artifacts)

	if result != builder {
		t.Error("WithArtifacts should return the same builder for chaining")
	}

	if len(builder.artifacts) != 2 {
		t.Errorf("expected 2 artifacts, got %d", len(builder.artifacts))
	}
}

func TestGoDevelopmentEnvironment_WithEnvironments(t *testing.T) {
	envs := []string{"FOO=bar", "BAZ=qux"}

	builder := NewGoDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithEnvironments(envs)

	if result != builder {
		t.Error("WithEnvironments should return the same builder for chaining")
	}

	if len(builder.environments) != 2 {
		t.Errorf("expected 2 environments, got %d", len(builder.environments))
	}
}

func TestGoDevelopmentEnvironment_WithSecrets(t *testing.T) {
	secrets := map[string]string{"key": "value"}

	builder := NewGoDevelopmentEnvironment("test-shell", testSystems)
	result := builder.WithSecrets(secrets)

	if result != builder {
		t.Error("WithSecrets should return the same builder for chaining")
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret, got %d", len(builder.secrets))
	}
}

func TestGoDevelopmentEnvironment_WithoutProtoc(t *testing.T) {
	builder := NewGoDevelopmentEnvironment("test-shell", testSystems)

	if !builder.includeProtoc || !builder.includeProtocGenGo || !builder.includeProtocGenGoGRPC {
		t.Fatal("protoc flags should be true before WithoutProtoc")
	}

	result := builder.WithoutProtoc()

	if result != builder {
		t.Error("WithoutProtoc should return the same builder for chaining")
	}

	if builder.includeProtoc {
		t.Error("expected includeProtoc to be false after WithoutProtoc")
	}

	if builder.includeProtocGenGo {
		t.Error("expected includeProtocGenGo to be false after WithoutProtoc")
	}

	if builder.includeProtocGenGoGRPC {
		t.Error("expected includeProtocGenGoGRPC to be false after WithoutProtoc")
	}
}

func TestGoDevelopmentEnvironment_Chaining(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}

	builder := NewGoDevelopmentEnvironment("test-shell", testSystems).
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

// Cross-builder tests

func TestAllBuilders_EmptySystems(t *testing.T) {
	goBuilder := NewGoDevelopmentEnvironment("test", []api.ArtifactSystem{})
	if len(goBuilder.systems) != 0 {
		t.Error("Go builder should accept empty systems")
	}

	rustBuilder := NewRustDevelopmentEnvironment("test", []api.ArtifactSystem{})
	if len(rustBuilder.systems) != 0 {
		t.Error("Rust builder should accept empty systems")
	}

	tsBuilder := NewTypeScriptDevelopmentEnvironment("test", []api.ArtifactSystem{})
	if len(tsBuilder.systems) != 0 {
		t.Error("TypeScript builder should accept empty systems")
	}
}

func TestAllBuilders_SingleSystem(t *testing.T) {
	systems := []api.ArtifactSystem{api.ArtifactSystem_AARCH64_DARWIN}

	goBuilder := NewGoDevelopmentEnvironment("test", systems)
	if len(goBuilder.systems) != 1 {
		t.Errorf("Go builder expected 1 system, got %d", len(goBuilder.systems))
	}

	rustBuilder := NewRustDevelopmentEnvironment("test", systems)
	if len(rustBuilder.systems) != 1 {
		t.Errorf("Rust builder expected 1 system, got %d", len(rustBuilder.systems))
	}

	tsBuilder := NewTypeScriptDevelopmentEnvironment("test", systems)
	if len(tsBuilder.systems) != 1 {
		t.Errorf("TypeScript builder expected 1 system, got %d", len(tsBuilder.systems))
	}
}
