import { ArtifactSystem } from "../api/artifact/artifact.js";
import { Artifact, getEnvKey } from "../artifact.js";
import type { ConfigContext } from "../context.js";
import { shell } from "./step.js";
import { Cargo } from "./cargo.js";
import { Clippy } from "./clippy.js";
import { RustAnalyzer } from "./rust_analyzer.js";
import { RustSrc } from "./rust_src.js";
import { RustStd } from "./rust_std.js";
import { Rustc } from "./rustc.js";
import { Rustfmt } from "./rustfmt.js";

import { rustToolchainTarget } from "./language/rust.js";
export { rustToolchainTarget } from "./language/rust.js";

export const RUST_TOOLCHAIN_VERSION = "1.93.1";

/**
 * Builder for the unified Rust toolchain artifact.
 *
 * Mirrors Rust `RustToolchain` struct in `sdk/rust/src/artifact/rust_toolchain.rs`.
 * Assembles all 7 sub-components (cargo, clippy, rust-analyzer, rust-src, rust-std,
 * rustc, rustfmt) into a single toolchain directory.
 */
export class RustToolchain {
  private _cargo: string | undefined;
  private _clippy: string | undefined;
  private _rustAnalyzer: string | undefined;
  private _rustSrc: string | undefined;
  private _rustStd: string | undefined;
  private _rustc: string | undefined;
  private _rustfmt: string | undefined;

  withCargo(cargo: string): this {
    this._cargo = cargo;
    return this;
  }

  withClippy(clippy: string): this {
    this._clippy = clippy;
    return this;
  }

  withRustAnalyzer(rustAnalyzer: string): this {
    this._rustAnalyzer = rustAnalyzer;
    return this;
  }

  withRustSrc(rustSrc: string): this {
    this._rustSrc = rustSrc;
    return this;
  }

  withRustStd(rustStd: string): this {
    this._rustStd = rustStd;
    return this;
  }

  withRustc(rustc: string): this {
    this._rustc = rustc;
    return this;
  }

  withRustfmt(rustfmt: string): this {
    this._rustfmt = rustfmt;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const cargo = this._cargo ?? await new Cargo().build(context);
    const clippy = this._clippy ?? await new Clippy().build(context);
    const rustAnalyzer = this._rustAnalyzer ?? await new RustAnalyzer().build(context);
    const rustSrc = this._rustSrc ?? await new RustSrc().build(context);
    const rustStd = this._rustStd ?? await new RustStd().build(context);
    const rustc = this._rustc ?? await new Rustc().build(context);
    const rustfmt = this._rustfmt ?? await new Rustfmt().build(context);

    const artifacts = [cargo, clippy, rustAnalyzer, rustSrc, rustStd, rustc, rustfmt];

    const componentPaths = artifacts.map((a) => getEnvKey(a)).join(" ");

    const toolchainTarget = rustToolchainTarget(context.getSystem());
    const toolchainVersion = RUST_TOOLCHAIN_VERSION;

    const stepScript = `toolchain_dir="$VORPAL_OUTPUT/toolchains/${toolchainVersion}-${toolchainTarget}"

mkdir -p "$toolchain_dir"

components=(${componentPaths})

echo "Copying Rust toolchain components to $toolchain_dir..."

for component in "\${components[@]}"; do
    echo "Processing component: $component"

    find "$component" | while read -r file; do
        relative_path=$(echo "$file" | sed -e "s|$component||")

        if [[ "$relative_path" == "/manifest.in" ]]; then
            continue
        fi

        if [ -d "$file" ]; then
            mkdir -p "$toolchain_dir$relative_path"
        else
            cp -p "$file" "$toolchain_dir$relative_path"
        fi
    done
done

cat > "$VORPAL_OUTPUT/settings.toml" << "EOF"
auto_self_update = "disable"
profile = "minimal"
version = "12"

[overrides]
EOF`;

    const steps = [await shell(context, artifacts, [], stepScript, [])];
    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];
    const name = "rust-toolchain";

    return new Artifact(name, steps, systems)
      .withAliases([`${name}:${toolchainVersion}`])
      .build(context);
  }
}
