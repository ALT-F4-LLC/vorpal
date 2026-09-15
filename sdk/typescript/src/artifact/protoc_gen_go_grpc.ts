import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";
import { SYSTEMS } from "../system.js";

/**
 * Builder for the protoc-gen-go-grpc artifact.
 *
 * Mirrors Rust `ProtocGenGoGrpc` struct in `sdk/rust/src/artifact/protoc_gen_go_grpc.rs`.
 * Builds protoc-gen-go-grpc from the grpc-go source using the Go language builder.
 */
export class ProtocGenGoGrpc {
  async build(context: ConfigContext): Promise<string> {
    const name = "protoc-gen-go-grpc";

    const sourceVersion = "1.79.1";
    const sourcePath = `https://sdk.vorpal.build/source/protoc-gen-go-grpc-v${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `grpc-go-${sourceVersion}/cmd/${name}`;

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withBuildDirectory(buildDirectory)
      .withSource(source)
      .build(context);
  }
}
