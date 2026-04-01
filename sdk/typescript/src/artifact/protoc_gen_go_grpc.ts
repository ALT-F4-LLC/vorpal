import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";

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
    const sourcePath = `https://github.com/grpc/grpc-go/archive/refs/tags/v${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `grpc-go-${sourceVersion}/cmd/${name}`;
    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:${sourceVersion}`])
      .withBuildDirectory(buildDirectory)
      .withSource(source)
      .build(context);
  }
}
