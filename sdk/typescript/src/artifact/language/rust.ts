import { existsSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { parse as parseToml } from "smol-toml";
import type { ArtifactStepSecret } from "../../api/artifact/artifact.js";
import { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  ArtifactBuilder,
  ArtifactSourceBuilder,
  getEnvKey,
} from "../../artifact.js";
import { shell } from "../step.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const RUST_TOOLCHAIN_VERSION = "1.89.0";
const PROTOC_ALIAS = "protoc:25.4";

// ---------------------------------------------------------------------------
// Cargo.toml parsing
// ---------------------------------------------------------------------------

/** Represents a `[[bin]]` entry in Cargo.toml. */
interface CargoTomlBinary {
  name: string;
  path: string;
}

/** Represents the `[package]` table in Cargo.toml. */
interface CargoTomlPackage {
  name: string;
}

/** Represents the `[workspace]` table in Cargo.toml. */
interface CargoTomlWorkspace {
  members: string[];
}

/** Minimal representation of a Cargo.toml file. */
interface CargoToml {
  bin: CargoTomlBinary[];
  package?: CargoTomlPackage;
  workspace?: CargoTomlWorkspace;
}

/**
 * Parses a Cargo.toml file using smol-toml.
 *
 * Extracts the fields the Rust builder needs:
 *   - `[package]` name
 *   - `[workspace]` members
 *   - `[[bin]]` name and path
 */
function parseCargo(path: string): CargoToml {
  const content = readFileSync(path, "utf-8");
  const doc = parseToml(content);

  const result: CargoToml = {
    bin: [],
    package: undefined,
    workspace: undefined,
  };

  // Extract [package] name
  if (
    doc.package !== undefined &&
    typeof doc.package === "object" &&
    doc.package !== null &&
    "name" in doc.package &&
    typeof (doc.package as Record<string, unknown>).name === "string"
  ) {
    result.package = {
      name: (doc.package as Record<string, unknown>).name as string,
    };
  }

  // Extract [workspace] members
  if (
    doc.workspace !== undefined &&
    typeof doc.workspace === "object" &&
    doc.workspace !== null &&
    "members" in doc.workspace
  ) {
    const ws = doc.workspace as Record<string, unknown>;
    if (Array.isArray(ws.members)) {
      result.workspace = {
        members: ws.members.filter(
          (m): m is string => typeof m === "string",
        ),
      };
    }
  }

  // Extract [[bin]] entries
  if (Array.isArray(doc.bin)) {
    for (const entry of doc.bin) {
      if (
        typeof entry === "object" &&
        entry !== null &&
        "name" in entry &&
        "path" in entry &&
        typeof (entry as Record<string, unknown>).name === "string" &&
        typeof (entry as Record<string, unknown>).path === "string"
      ) {
        result.bin.push({
          name: (entry as Record<string, unknown>).name as string,
          path: (entry as Record<string, unknown>).path as string,
        });
      }
    }
  }

  return result;
}

// ---------------------------------------------------------------------------
// Rust toolchain target mapping
// ---------------------------------------------------------------------------

/**
 * Maps an ArtifactSystem enum to the Rust target triple.
 * Matches `sdk/go/pkg/artifact/rust_toolchain.go` RustToolchainTarget().
 */
function rustToolchainTarget(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
      return "aarch64-apple-darwin";
    case ArtifactSystem.AARCH64_LINUX:
      return "aarch64-unknown-linux-gnu";
    case ArtifactSystem.X8664_DARWIN:
      return "x86_64-apple-darwin";
    case ArtifactSystem.X8664_LINUX:
      return "x86_64-unknown-linux-gnu";
    default:
      throw new Error(`unsupported 'rust-toolchain' system: ${system}`);
  }
}

// ---------------------------------------------------------------------------
// Shell script helpers
// ---------------------------------------------------------------------------

/** Builds the vendor step script for `cargo vendor`. */
function buildVendorScript(
  name: string,
  packages: string[],
  packagesTargets: string[],
): string {
  const lines: string[] = [];

  lines.push(`mkdir -pv $HOME`);
  lines.push(``);
  lines.push(`pushd ./source/${name}-vendor`);

  if (packages.length > 0) {
    const quotedPackages = packages.map((p) => `"${p}"`).join(",");
    const quotedTargets = packagesTargets.map((t) => `"${t}"`).join(" ");

    lines.push(``);
    lines.push(`cat > Cargo.toml << "EOF"`);
    lines.push(`[workspace]`);
    lines.push(`members = [${quotedPackages}]`);
    lines.push(`resolver = "2"`);
    lines.push(`EOF`);

    lines.push(``);
    lines.push(`target_paths=(${quotedTargets})`);
    lines.push(``);
    lines.push(`for target_path in \${target_paths[@]}; do`);
    lines.push(`    mkdir -pv $(dirname \${target_path})`);
    lines.push(`    touch \${target_path}`);
    lines.push(`done`);
  } else {
    lines.push(``);
    lines.push(`mkdir -pv src`);
    lines.push(`touch src/main.rs`);
  }

  lines.push(``);
  lines.push(`mkdir -pv $VORPAL_OUTPUT/vendor`);
  lines.push(``);
  lines.push(
    `cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)`,
  );
  lines.push(``);
  lines.push(`echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml`);

  return lines.join("\n");
}

/** Builds the main build step script for `cargo build --release`. */
function buildMainScript(opts: {
  name: string;
  vendorDigest: string;
  packages: string[];
  binNames: string[];
  manifests: string[];
  format: boolean;
  lint: boolean;
  check: boolean;
  build: boolean;
  tests: boolean;
}): string {
  const lines: string[] = [];

  lines.push(`mkdir -pv $HOME`);
  lines.push(``);
  lines.push(`pushd ./source/${opts.name}`);
  lines.push(``);
  lines.push(`mkdir -pv .cargo`);
  lines.push(`mkdir -pv $VORPAL_OUTPUT/bin`);
  lines.push(``);
  lines.push(
    `ln -sv ${getEnvKey(opts.vendorDigest)}/config.toml .cargo/config.toml`,
  );

  if (opts.packages.length > 0) {
    const quotedPackages = opts.packages.map((p) => `"${p}"`).join(",");
    lines.push(``);
    lines.push(`cat > Cargo.toml << "EOF"`);
    lines.push(`[workspace]`);
    lines.push(`members = [${quotedPackages}]`);
    lines.push(`resolver = "2"`);
    lines.push(`EOF`);
  }

  lines.push(``);
  lines.push(`bin_names=(${opts.binNames.join(" ")})`);
  lines.push(`manifest_paths=(${opts.manifests.join(" ")})`);

  if (opts.format) {
    lines.push(``);
    lines.push(`echo "Running formatter..."`);
    lines.push(`cargo --offline fmt --all --check`);
  }

  if (opts.lint) {
    lines.push(``);
    lines.push(`for manifest_path in \${manifest_paths[@]}; do`);
    lines.push(`    echo "Running linter..."`);
    lines.push(
      `    cargo --offline clippy --manifest-path \${manifest_path} -- --deny warnings`,
    );
    lines.push(`done`);
  }

  lines.push(``);
  lines.push(`for bin_name in \${bin_names[@]}; do`);

  if (opts.check) {
    lines.push(`    echo "Running check..."`);
    lines.push(`    cargo --offline check --bin \${bin_name} --release`);
  }

  if (opts.build) {
    lines.push(`    echo "Running build..."`);
    lines.push(`    cargo --offline build --bin \${bin_name} --release`);
  }

  if (opts.tests) {
    lines.push(`    echo "Running tests..."`);
    lines.push(`    cargo --offline test --bin \${bin_name} --release`);
  }

  lines.push(`    cp -pv ./target/release/\${bin_name} $VORPAL_OUTPUT/bin/`);
  lines.push(`done`);

  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// RustBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for Rust/Cargo project artifacts.
 *
 * Analogous to:
 * - Rust SDK: `sdk/rust/src/artifact/language/rust.rs` (`Rust` struct)
 * - Go SDK: `sdk/go/pkg/artifact/language/rust.go` (`Rust` struct)
 *
 * The builder:
 * 1. Fetches the rust-toolchain artifact from the registry
 * 2. Fetches the protoc artifact from the registry
 * 3. Parses the Cargo.toml workspace to identify packages, bins, and targets
 * 4. Creates a vendor artifact that runs `cargo vendor`
 * 5. Creates the main build artifact with `cargo build --release`
 *
 * Usage:
 * ```typescript
 * const digest = await new RustBuilder("vorpal", SYSTEMS)
 *   .withPackages(["vorpal-cli"])
 *   .withBins(["vorpal"])
 *   .build(context);
 * ```
 */
export class RustBuilder {
  private _artifacts: string[] = [];
  private _bins: string[] = [];
  private _build: boolean = true;
  private _check: boolean = false;
  private _environments: string[] = [];
  private _excludes: string[] = [];
  private _format: boolean = false;
  private _includes: string[] = [];
  private _lint: boolean = false;
  private _name: string;
  private _packages: string[] = [];
  private _secrets: ArtifactStepSecret[] = [];
  private _source: string | undefined = undefined;
  private _systems: ArtifactSystem[];
  private _tests: boolean = false;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /** Adds artifact dependencies available during the build step. */
  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  /** Sets the binary targets to build. If empty, defaults to the builder name. */
  withBins(bins: string[]): this {
    this._bins = bins;
    return this;
  }

  /** Enables `cargo check` during the build step. */
  withCheck(): this {
    this._check = true;
    return this;
  }

  /** Sets additional environment variables for the build step. Format: "KEY=VALUE". */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  /** Sets file patterns to exclude from the source. */
  withExcludes(excludes: string[]): this {
    this._excludes = excludes;
    return this;
  }

  /** Enables or disables `cargo fmt --check` during the build step. */
  withFormat(format: boolean): this {
    this._format = format;
    return this;
  }

  /** Sets file patterns to include in the source. */
  withIncludes(includes: string[]): this {
    this._includes = includes;
    return this;
  }

  /** Enables or disables `cargo clippy` during the build step. */
  withLint(lint: boolean): this {
    this._lint = lint;
    return this;
  }

  /** Filters which workspace packages to include in the build. */
  withPackages(packages: string[]): this {
    this._packages = packages;
    return this;
  }

  /**
   * Adds secrets available during the build step.
   * Secrets are deduplicated by name.
   */
  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some((s) => s.name === name)) {
        this._secrets.push({ name, value });
      }
    }
    return this;
  }

  /** Sets the source path relative to the artifact context directory. */
  withSource(source: string): this {
    this._source = source;
    return this;
  }

  /** Enables or disables `cargo test` during the build step. */
  withTests(tests: boolean): this {
    this._tests = tests;
    return this;
  }

  /**
   * Builds the Rust project artifact.
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
    // Sort secrets for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    // Fetch protoc artifact
    const protoc = await context.fetchArtifactAlias(PROTOC_ALIAS);

    // Parse source path
    const contextPath = context.getArtifactContextPath();
    const sourcePath = this._source ?? ".";
    const contextPathSource = join(contextPath, sourcePath);

    if (!existsSync(contextPathSource)) {
      throw new Error(
        `\`source.${this._name}.path\` not found: ${sourcePath}`,
      );
    }

    // Parse Cargo.toml
    const sourceCargoPath = join(contextPathSource, "Cargo.toml");

    if (!existsSync(sourceCargoPath)) {
      throw new Error(`Cargo.toml not found: ${sourceCargoPath}`);
    }

    const sourceCargo = parseCargo(sourceCargoPath);

    // Get list of bin targets
    const packages: string[] = [];
    const packagesBinNames: string[] = [];
    const packagesManifests: string[] = [];
    const packagesTargets: string[] = [];

    if (
      sourceCargo.workspace !== undefined &&
      sourceCargo.workspace.members.length > 0
    ) {
      for (const member of sourceCargo.workspace.members) {
        const packagePath = join(contextPathSource, member);
        const packageCargoPath = join(packagePath, "Cargo.toml");

        if (!existsSync(packageCargoPath)) {
          throw new Error(`Cargo.toml not found: ${packageCargoPath}`);
        }

        const packageCargo = parseCargo(packageCargoPath);

        if (
          this._packages.length > 0 &&
          packageCargo.package !== undefined &&
          !this._packages.includes(packageCargo.package.name)
        ) {
          continue;
        }

        const packageTargetPaths: string[] = [];

        if (packageCargo.bin.length > 0) {
          for (const bin of packageCargo.bin) {
            const packageTargetPath = join(packagePath, bin.path);

            if (!existsSync(packageTargetPath)) {
              throw new Error(
                `bin target not found: ${packageTargetPath}`,
              );
            }

            packageTargetPaths.push(packageTargetPath);

            if (
              this._bins.length === 0 ||
              this._bins.includes(bin.name)
            ) {
              if (!packagesManifests.includes(packageCargoPath)) {
                packagesManifests.push(packageCargoPath);
              }

              packagesBinNames.push(bin.name);
            }
          }
        }

        if (packageTargetPaths.length === 0) {
          const packageTargetPath = join(packagePath, "src/lib.rs");

          if (!existsSync(packageTargetPath)) {
            throw new Error(
              `lib.rs not found: ${packageTargetPath}`,
            );
          }

          packageTargetPaths.push(packageTargetPath);
        }

        for (const memberTargetPath of packageTargetPaths) {
          const memberTargetPathRelative = relative(
            contextPathSource,
            memberTargetPath,
          );
          packagesTargets.push(memberTargetPathRelative);
        }

        packages.push(member);
      }
    }

    // 2. CREATE ARTIFACTS

    // Get rust toolchain artifact
    const rustToolchain = await context.fetchArtifactAlias(
      `rust-toolchain:${RUST_TOOLCHAIN_VERSION}`,
    );

    const rustToolchainTargetStr = rustToolchainTarget(context.getSystem());
    const rustToolchainName = `${RUST_TOOLCHAIN_VERSION}-${rustToolchainTargetStr}`;

    const stepEnvironments: string[] = [
      "HOME=$VORPAL_WORKSPACE/home",
      `PATH=${getEnvKey(rustToolchain)}/toolchains/${rustToolchainName}/bin`,
      `RUSTUP_HOME=${getEnvKey(rustToolchain)}`,
      `RUSTUP_TOOLCHAIN=${rustToolchainName}`,
    ];

    for (const env of this._environments) {
      stepEnvironments.push(env);
    }

    // Create vendor artifact

    const vendorCargoPaths: string[] = ["Cargo.toml", "Cargo.lock"];

    for (const pkg of packages) {
      vendorCargoPaths.push(`${pkg}/Cargo.toml`);
    }

    const vendorStepScript = buildVendorScript(
      this._name,
      packages,
      packagesTargets,
    );

    let stepArtifacts: string[] = [rustToolchain];

    const vendorStep = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      vendorStepScript,
      this._secrets,
    );

    const vendorName = `${this._name}-vendor`;

    const vendorSource = new ArtifactSourceBuilder(vendorName, sourcePath)
      .withIncludes(vendorCargoPaths)
      .build();

    const vendor = await new ArtifactBuilder(
      vendorName,
      [vendorStep],
      this._systems,
    )
      .withSources([vendorSource])
      .build(context);

    stepArtifacts = [...stepArtifacts, vendor, protoc];

    // Create source

    const sourceIncludes: string[] = [];
    const sourceExcludes: string[] = ["target"];

    for (const exclude of this._excludes) {
      sourceExcludes.push(exclude);
    }

    for (const include of this._includes) {
      sourceIncludes.push(include);
    }

    const sourceBuilder = new ArtifactSourceBuilder(this._name, sourcePath)
      .withIncludes(sourceIncludes)
      .withExcludes(sourceExcludes);

    const source = sourceBuilder.build();

    for (const artifact of this._artifacts) {
      stepArtifacts.push(artifact);
    }

    // Create step

    if (packagesBinNames.length === 0) {
      packagesBinNames.push(this._name);
    }

    if (packagesManifests.length === 0) {
      packagesManifests.push(sourceCargoPath);
    }

    const stepScript = buildMainScript({
      name: this._name,
      vendorDigest: vendor,
      packages,
      binNames: packagesBinNames,
      manifests: packagesManifests,
      format: this._format,
      lint: this._lint,
      check: this._check,
      build: this._build,
      tests: this._tests,
    });

    const step = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      stepScript,
      this._secrets,
    );

    // Create artifact

    return new ArtifactBuilder(this._name, [step], this._systems)
      .withSources([source])
      .build(context);
  }
}
