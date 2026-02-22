import { describe, expect, test } from "bun:test";
import { parseArtifactAlias, formatArtifactAlias } from "../context.js";

// ---------------------------------------------------------------------------
// Helper functions matching Rust test helpers
// ---------------------------------------------------------------------------

function assertAlias(
  input: string,
  expectedName: string,
  expectedNamespace: string,
  expectedTag: string,
) {
  const result = parseArtifactAlias(input);
  expect(result.name).toBe(expectedName);
  expect(result.namespace).toBe(expectedNamespace);
  expect(result.tag).toBe(expectedTag);
}

function assertAliasError(input: string, expectedSubstring: string) {
  expect(() => parseArtifactAlias(input)).toThrow(expectedSubstring);
}

// ---------------------------------------------------------------------------
// Tests — ported from Rust sdk/rust/src/context.rs lines 712-1017
// and Go sdk/go/pkg/config/context_test.go
// ---------------------------------------------------------------------------

describe("alias parsing", () => {
  // -----------------------------------------------------------
  // Basic formats (ported from Go TestParseArtifactAlias)
  // -----------------------------------------------------------

  test("name only", () => {
    assertAlias("myapp", "myapp", "library", "latest");
  });

  test("name with tag", () => {
    assertAlias("myapp:1.0.0", "myapp", "library", "1.0.0");
  });

  test("namespace and name", () => {
    assertAlias("team/myapp", "myapp", "team", "latest");
  });

  test("full format", () => {
    assertAlias("team/myapp:v2.1", "myapp", "team", "v2.1");
  });

  // -----------------------------------------------------------
  // Real-world examples from codebase
  // -----------------------------------------------------------

  test("linux-vorpal:latest", () => {
    assertAlias("linux-vorpal:latest", "linux-vorpal", "library", "latest");
  });

  test("gh:2.69.0", () => {
    assertAlias("gh:2.69.0", "gh", "library", "2.69.0");
  });

  test("protoc:25.4", () => {
    assertAlias("protoc:25.4", "protoc", "library", "25.4");
  });

  test("protoc-gen-go:1.36.3", () => {
    assertAlias("protoc-gen-go:1.36.3", "protoc-gen-go", "library", "1.36.3");
  });

  // -----------------------------------------------------------
  // Edge cases - multiple colons
  // Rust: rejects because name="name:tag" contains invalid colon
  // -----------------------------------------------------------

  test("name with multiple colons rejected", () => {
    // After rightmost-colon split, name="name:tag" which contains an invalid colon
    assertAliasError("name:tag:extra", "name contains invalid characters");
  });

  // -----------------------------------------------------------
  // Names with special characters
  // -----------------------------------------------------------

  test("name with hyphens", () => {
    assertAlias("my-app-name:v1.0", "my-app-name", "library", "v1.0");
  });

  test("name with underscores", () => {
    assertAlias("my_app_name:v1.0", "my_app_name", "library", "v1.0");
  });

  test("namespace with hyphens", () => {
    assertAlias("my-team/my-app:v1.0", "my-app", "my-team", "v1.0");
  });

  // -----------------------------------------------------------
  // Semantic versions
  // -----------------------------------------------------------

  test("semantic version tag", () => {
    assertAlias("myapp:1.2.3", "myapp", "library", "1.2.3");
  });

  test("semantic version with v prefix", () => {
    assertAlias("myapp:v1.2.3", "myapp", "library", "v1.2.3");
  });

  // -----------------------------------------------------------
  // Numeric components
  // -----------------------------------------------------------

  test("numeric name", () => {
    assertAlias("123:latest", "123", "library", "latest");
  });

  test("numeric namespace", () => {
    assertAlias("123/myapp:v1.0", "myapp", "123", "v1.0");
  });

  // -----------------------------------------------------------
  // Error cases
  // -----------------------------------------------------------

  test("error: empty string", () => {
    assertAliasError("", "alias cannot be empty");
  });

  test("error: empty tag", () => {
    assertAliasError("name:", "tag cannot be empty");
  });

  test("error: too many slashes", () => {
    assertAliasError("a/b/c", "too many path separators");
  });

  test("error: empty namespace before slash", () => {
    assertAliasError("/name", "namespace cannot be empty");
  });

  test("error: empty name after slash", () => {
    assertAliasError("namespace/", "name is required");
  });

  test("error: too long alias", () => {
    const longAlias = "a".repeat(256);
    assertAliasError(longAlias, "alias too long");
  });

  test("error: only slash", () => {
    assertAliasError("/", "namespace cannot be empty");
  });

  test("error: only colon", () => {
    assertAliasError(":", "tag cannot be empty");
  });

  // -----------------------------------------------------------
  // Default value application (ported from Go TestParseArtifactAliasDefaults
  // and Rust test_defaults_*)
  // -----------------------------------------------------------

  test("defaults: both applied", () => {
    const result = parseArtifactAlias("myapp");
    expect(result.tag).toBe("latest");
    expect(result.namespace).toBe("library");
  });

  test("defaults: only tag", () => {
    const result = parseArtifactAlias("team/myapp");
    expect(result.tag).toBe("latest");
    expect(result.namespace).toBe("team");
  });

  test("defaults: only namespace", () => {
    const result = parseArtifactAlias("myapp:v1.0");
    expect(result.tag).toBe("v1.0");
    expect(result.namespace).toBe("library");
  });

  test("defaults: none applied", () => {
    const result = parseArtifactAlias("team/myapp:v1.0");
    expect(result.tag).toBe("v1.0");
    expect(result.namespace).toBe("team");
  });

  // -----------------------------------------------------------
  // Character validation (ported from Rust test_valid_* and test_error_*)
  // -----------------------------------------------------------

  test("valid characters with plus sign", () => {
    assertAlias(
      "valid-name_1.0+build:v2.3",
      "valid-name_1.0+build",
      "library",
      "v2.3",
    );
  });

  test("valid semver with prerelease and build", () => {
    assertAlias(
      "my-namespace/my-artifact:v1.2.3-beta+build.123",
      "my-artifact",
      "my-namespace",
      "v1.2.3-beta+build.123",
    );
  });

  test("error: path traversal multi-slash", () => {
    assertAliasError("../../etc:passwd", "too many path separators");
  });

  test("error: whitespace in name", () => {
    assertAliasError("name with spaces:tag", "name contains invalid characters");
  });

  test("error: whitespace in namespace", () => {
    assertAliasError("bad ns/name:tag", "namespace contains invalid characters");
  });

  test("error: special chars in tag", () => {
    assertAliasError("name:tag@sha256", "tag contains invalid characters");
  });

  test("error: control chars in name", () => {
    assertAliasError("name\x00bad", "name contains invalid characters");
  });

  test("error: shell metachar in name", () => {
    assertAliasError("name;echo:tag", "name contains invalid characters");
  });

  test("error: tilde in namespace", () => {
    assertAliasError("~root/app:v1", "namespace contains invalid characters");
  });

  test("error: backslash in name", () => {
    assertAliasError("name\\bad:tag", "name contains invalid characters");
  });

  // -----------------------------------------------------------
  // Additional edge cases
  // -----------------------------------------------------------

  test("single character name", () => {
    assertAlias("a", "a", "library", "latest");
  });

  test("single character tag", () => {
    assertAlias("app:1", "app", "library", "1");
  });

  test("single character namespace and name", () => {
    assertAlias("n/a", "a", "n", "latest");
  });

  test("max length alias (255 chars)", () => {
    const name = "a".repeat(255);
    const result = parseArtifactAlias(name);
    expect(result.name).toBe(name);
    expect(result.namespace).toBe("library");
    expect(result.tag).toBe("latest");
  });

  test("256 chars is too long", () => {
    assertAliasError("a".repeat(256), "alias too long");
  });

  test("dots in name", () => {
    assertAlias("my.app:v1.0", "my.app", "library", "v1.0");
  });

  test("uppercase in components", () => {
    assertAlias("MyApp:V1", "MyApp", "library", "V1");
  });

  test("mixed case namespace", () => {
    assertAlias("MyTeam/MyApp:v1", "MyApp", "MyTeam", "v1");
  });
});

// ---------------------------------------------------------------------------
// formatArtifactAlias round-trip tests
// ---------------------------------------------------------------------------

describe("alias formatting", () => {
  test("format name only (defaults omitted)", () => {
    const alias = parseArtifactAlias("myapp");
    expect(formatArtifactAlias(alias)).toBe("myapp");
  });

  test("format name with tag", () => {
    const alias = parseArtifactAlias("myapp:1.0.0");
    expect(formatArtifactAlias(alias)).toBe("myapp:1.0.0");
  });

  test("format namespace and name", () => {
    const alias = parseArtifactAlias("team/myapp");
    expect(formatArtifactAlias(alias)).toBe("team/myapp");
  });

  test("format full alias", () => {
    const alias = parseArtifactAlias("team/myapp:v2.1");
    expect(formatArtifactAlias(alias)).toBe("team/myapp:v2.1");
  });

  test("format omits default namespace", () => {
    const alias = parseArtifactAlias("myapp:v1.0");
    expect(formatArtifactAlias(alias)).toBe("myapp:v1.0");
    expect(alias.namespace).toBe("library");
  });

  test("format omits default tag", () => {
    const alias = parseArtifactAlias("team/myapp");
    expect(formatArtifactAlias(alias)).toBe("team/myapp");
    expect(alias.tag).toBe("latest");
  });

  test("round-trip: name only", () => {
    const input = "myapp";
    expect(formatArtifactAlias(parseArtifactAlias(input))).toBe(input);
  });

  test("round-trip: name with tag", () => {
    const input = "gh:2.69.0";
    expect(formatArtifactAlias(parseArtifactAlias(input))).toBe(input);
  });

  test("round-trip: namespace and name", () => {
    const input = "team/myapp";
    expect(formatArtifactAlias(parseArtifactAlias(input))).toBe(input);
  });

  test("round-trip: full format", () => {
    const input = "team/myapp:v2.1";
    expect(formatArtifactAlias(parseArtifactAlias(input))).toBe(input);
  });

  test("round-trip: semver with prerelease", () => {
    const input = "my-namespace/my-artifact:v1.2.3-beta+build.123";
    expect(formatArtifactAlias(parseArtifactAlias(input))).toBe(input);
  });
});
