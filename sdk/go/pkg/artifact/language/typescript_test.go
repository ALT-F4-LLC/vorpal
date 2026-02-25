package language

import (
	"testing"
)

// TypeScript builder tests

func TestNewTypeScript(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems)

	if builder == nil {
		t.Fatal("expected non-nil builder")
	}

	if builder.name != "test-ts" {
		t.Errorf("expected name %q, got %q", "test-ts", builder.name)
	}

	if len(builder.systems) != len(testSystems) {
		t.Errorf("expected %d systems, got %d", len(testSystems), len(builder.systems))
	}

	if len(builder.aliases) != 0 {
		t.Errorf("expected empty aliases, got %d", len(builder.aliases))
	}

	if len(builder.artifacts) != 0 {
		t.Errorf("expected empty artifacts, got %d", len(builder.artifacts))
	}

	if builder.entrypoint != nil {
		t.Errorf("expected nil entrypoint, got %v", builder.entrypoint)
	}

	if len(builder.environments) != 0 {
		t.Errorf("expected empty environments, got %d", len(builder.environments))
	}

	if len(builder.includes) != 0 {
		t.Errorf("expected empty includes, got %d", len(builder.includes))
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

	if len(builder.sourceScripts) != 0 {
		t.Errorf("expected empty sourceScripts, got %d", len(builder.sourceScripts))
	}

	if !builder.vorpalSdk {
		t.Error("expected vorpalSdk to be true by default")
	}

	if builder.workingDir != nil {
		t.Errorf("expected nil workingDir, got %v", builder.workingDir)
	}
}

func TestTypeScript_WithAliases(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithAliases([]string{"alias-a", "alias-b"})

	if result != builder {
		t.Error("WithAliases should return the same builder for chaining")
	}

	if len(builder.aliases) != 2 {
		t.Errorf("expected 2 aliases, got %d", len(builder.aliases))
	}

	if builder.aliases[0] != "alias-a" || builder.aliases[1] != "alias-b" {
		t.Errorf("unexpected aliases: %v", builder.aliases)
	}
}

func TestTypeScript_WithAliases_Dedup(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems).
		WithAliases([]string{"alias-a", "alias-b"}).
		WithAliases([]string{"alias-b", "alias-c"})

	if len(builder.aliases) != 3 {
		t.Errorf("expected 3 aliases after dedup, got %d: %v", len(builder.aliases), builder.aliases)
	}
}

func TestTypeScript_WithArtifacts(t *testing.T) {
	a := "digest-1"
	b := "digest-2"
	artifacts := []*string{&a, &b}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithArtifacts(artifacts)

	if result != builder {
		t.Error("WithArtifacts should return the same builder for chaining")
	}

	if len(builder.artifacts) != 2 {
		t.Errorf("expected 2 artifacts, got %d", len(builder.artifacts))
	}
}

func TestTypeScript_WithEntrypoint(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithEntrypoint("src/main.ts")

	if result != builder {
		t.Error("WithEntrypoint should return the same builder for chaining")
	}

	if builder.entrypoint == nil {
		t.Fatal("expected entrypoint to be set")
	}

	if *builder.entrypoint != "src/main.ts" {
		t.Errorf("expected entrypoint %q, got %q", "src/main.ts", *builder.entrypoint)
	}
}

func TestTypeScript_WithEnvironments(t *testing.T) {
	envs := []string{"FOO=bar", "BAZ=qux"}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithEnvironments(envs)

	if result != builder {
		t.Error("WithEnvironments should return the same builder for chaining")
	}

	if len(builder.environments) != 2 {
		t.Errorf("expected 2 environments, got %d", len(builder.environments))
	}
}

func TestTypeScript_WithIncludes(t *testing.T) {
	includes := []string{"src/**/*.ts", "package.json"}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithIncludes(includes)

	if result != builder {
		t.Error("WithIncludes should return the same builder for chaining")
	}

	if len(builder.includes) != 2 {
		t.Errorf("expected 2 includes, got %d", len(builder.includes))
	}
}

func TestTypeScript_WithNodeModules(t *testing.T) {
	d1 := "digest-aaa"
	d2 := "digest-bbb"

	modules := map[string]*string{
		"@vorpal/sdk": &d1,
		"lodash":      &d2,
	}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithNodeModules(modules)

	if result != builder {
		t.Error("WithNodeModules should return the same builder for chaining")
	}

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

func TestTypeScript_WithSecrets(t *testing.T) {
	secrets := map[string]string{"key": "value"}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithSecrets(secrets)

	if result != builder {
		t.Error("WithSecrets should return the same builder for chaining")
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret, got %d", len(builder.secrets))
	}
}

func TestTypeScript_WithSecrets_Dedup(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems).
		WithSecrets(map[string]string{"key": "value1"}).
		WithSecrets(map[string]string{"key": "value2"})

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret after dedup, got %d", len(builder.secrets))
	}

	if builder.secrets[0].Value != "value1" {
		t.Errorf("expected first secret value to be preserved, got %q", builder.secrets[0].Value)
	}
}

func TestTypeScript_WithSourceScripts(t *testing.T) {
	scripts := []string{"echo hello", "echo world"}

	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithSourceScripts(scripts)

	if result != builder {
		t.Error("WithSourceScripts should return the same builder for chaining")
	}

	if len(builder.sourceScripts) != 2 {
		t.Errorf("expected 2 source scripts, got %d", len(builder.sourceScripts))
	}
}

func TestTypeScript_WithSourceScripts_Dedup(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems).
		WithSourceScripts([]string{"echo hello", "echo world"}).
		WithSourceScripts([]string{"echo world", "echo new"})

	if len(builder.sourceScripts) != 3 {
		t.Errorf("expected 3 source scripts after dedup, got %d: %v", len(builder.sourceScripts), builder.sourceScripts)
	}
}

func TestTypeScript_WithVorpalSdk(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems)

	if !builder.vorpalSdk {
		t.Fatal("expected vorpalSdk to be true by default")
	}

	result := builder.WithVorpalSdk(false)

	if result != builder {
		t.Error("WithVorpalSdk should return the same builder for chaining")
	}

	if builder.vorpalSdk {
		t.Error("expected vorpalSdk to be false after WithVorpalSdk(false)")
	}
}

func TestTypeScript_WithWorkingDir(t *testing.T) {
	builder := NewTypeScript("test-ts", testSystems)
	result := builder.WithWorkingDir("packages/core")

	if result != builder {
		t.Error("WithWorkingDir should return the same builder for chaining")
	}

	if builder.workingDir == nil {
		t.Fatal("expected workingDir to be set")
	}

	if *builder.workingDir != "packages/core" {
		t.Errorf("expected workingDir %q, got %q", "packages/core", *builder.workingDir)
	}
}

func TestTypeScript_Chaining(t *testing.T) {
	a := "digest-1"
	artifacts := []*string{&a}
	d1 := "node-digest-1"

	builder := NewTypeScript("test-ts", testSystems).
		WithAliases([]string{"my-alias"}).
		WithArtifacts(artifacts).
		WithEntrypoint("src/main.ts").
		WithEnvironments([]string{"FOO=bar"}).
		WithIncludes([]string{"src/**"}).
		WithNodeModules(map[string]*string{"@vorpal/sdk": &d1}).
		WithSecrets(map[string]string{"key": "value"}).
		WithSourceScripts([]string{"echo setup"}).
		WithVorpalSdk(false).
		WithWorkingDir("packages/app")

	if len(builder.aliases) != 1 {
		t.Errorf("expected 1 alias after chaining, got %d", len(builder.aliases))
	}

	if len(builder.artifacts) != 1 {
		t.Errorf("expected 1 artifact after chaining, got %d", len(builder.artifacts))
	}

	if builder.entrypoint == nil || *builder.entrypoint != "src/main.ts" {
		t.Error("expected entrypoint to be set after chaining")
	}

	if len(builder.environments) != 1 {
		t.Errorf("expected 1 environment after chaining, got %d", len(builder.environments))
	}

	if len(builder.includes) != 1 {
		t.Errorf("expected 1 include after chaining, got %d", len(builder.includes))
	}

	if len(builder.nodeModules) != 1 {
		t.Errorf("expected 1 node module after chaining, got %d", len(builder.nodeModules))
	}

	if len(builder.secrets) != 1 {
		t.Errorf("expected 1 secret after chaining, got %d", len(builder.secrets))
	}

	if len(builder.sourceScripts) != 1 {
		t.Errorf("expected 1 source script after chaining, got %d", len(builder.sourceScripts))
	}

	if builder.vorpalSdk {
		t.Error("expected vorpalSdk to be false after chaining")
	}

	if builder.workingDir == nil || *builder.workingDir != "packages/app" {
		t.Error("expected workingDir to be set after chaining")
	}
}
