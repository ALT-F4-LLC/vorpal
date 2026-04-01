import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";
import { Protoc } from "./protoc.js";

/**
 * Builder for the grpcurl artifact.
 *
 * Mirrors Rust `Grpcurl` struct in `sdk/rust/src/artifact/grpcurl.rs`.
 * Builds grpcurl from source using the Go language builder.
 * Depends on protoc as an artifact dependency.
 */
export class Grpcurl {
  private _protoc: string | undefined = undefined;

  withProtoc(protoc: string): this {
    this._protoc = protoc;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    let protoc: string;

    if (this._protoc !== undefined) {
      protoc = this._protoc;
    } else {
      protoc = await new Protoc().build(context);
    }

    const name = "grpcurl";

    const sourceVersion = "1.9.3";
    const sourcePath = `https://github.com/fullstorydev/grpcurl/archive/refs/tags/v${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `${name}-${sourceVersion}`;
    const buildPath = `cmd/${name}/${name}.go`;

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:${sourceVersion}`])
      .withArtifacts([protoc])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
