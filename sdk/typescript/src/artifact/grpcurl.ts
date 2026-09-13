import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";
import { Protoc } from "./protoc.js";
import { SYSTEMS } from "../system.js";

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
    const sourcePath = `https://sdk.vorpal.build/source/grpcurl-v${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `${name}-${sourceVersion}`;
    const buildPath = `cmd/${name}/${name}.go`;

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withArtifacts([protoc])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
