import { ArtifactSystem } from "./api/artifact/artifact.js";
import {
  getEnvKey,
  JobBuilder,
  OciImageBuilder,
  ProcessBuilder,
  ProjectEnvironmentBuilder,
  UserEnvironmentBuilder,
} from "./artifact.js";
import { RustBuilder } from "./artifact/language/rust.js";
import { ConfigContext } from "./context.js";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

function getGoarch(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
    case ArtifactSystem.AARCH64_LINUX:
      return "arm64";
    case ArtifactSystem.X8664_DARWIN:
    case ArtifactSystem.X8664_LINUX:
      return "amd64";
    default:
      throw new Error(`unsupported system for GOARCH: ${system}`);
  }
}

function getGoos(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
    case ArtifactSystem.X8664_DARWIN:
      return "darwin";
    case ArtifactSystem.AARCH64_LINUX:
    case ArtifactSystem.X8664_LINUX:
      return "linux";
    default:
      throw new Error(`unsupported system for GOOS: ${system}`);
  }
}

async function buildVorpal(context: ConfigContext): Promise<string> {
  return new RustBuilder("vorpal", SYSTEMS)
    .withBins(["vorpal"])
    .withIncludes(["cli", "sdk/rust"])
    .withPackages(["vorpal-cli", "vorpal-sdk"])
    .build(context);
}

async function buildVorpalContainerImage(
  context: ConfigContext,
): Promise<string> {
  const linuxVorpalSlim = await context.fetchArtifactAlias(
    "linux-vorpal-slim:latest",
  );
  const vorpal = await buildVorpal(context);

  const name = "vorpal-container-image";

  return new OciImageBuilder(name, linuxVorpalSlim)
    .withAliases([`${name}:latest`])
    .withArtifacts([vorpal])
    .build(context);
}

async function buildVorpalJob(context: ConfigContext): Promise<string> {
  const vorpal = await buildVorpal(context);
  const script = `${getEnvKey(vorpal)}/bin/vorpal --version`;

  return new JobBuilder("vorpal-job", script, SYSTEMS)
    .withArtifacts([vorpal])
    .build(context);
}

async function buildVorpalProcess(context: ConfigContext): Promise<string> {
  const vorpal = await buildVorpal(context);

  return new ProcessBuilder(
    "vorpal-process",
    `${getEnvKey(vorpal)}/bin/vorpal`,
    SYSTEMS,
  )
    .withArguments([
      "--registry",
      "https://localhost:50051",
      "services",
      "start",
      "--port",
      "50051",
    ])
    .withArtifacts([vorpal])
    .build(context);
}

async function buildVorpalShell(context: ConfigContext): Promise<string> {
  const bun = await context.fetchArtifactAlias("bun:1.2.0");
  const crane = await context.fetchArtifactAlias("crane:0.20.7");
  const go = await context.fetchArtifactAlias("go:1.24.2");
  const goimports = await context.fetchArtifactAlias("goimports:0.29.0");
  const gopls = await context.fetchArtifactAlias("gopls:0.29.0");
  const grpcurl = await context.fetchArtifactAlias("grpcurl:1.9.3");
  const nodejs = await context.fetchArtifactAlias("nodejs:22.14.0");
  const pnpm = await context.fetchArtifactAlias("pnpm:10.5.2");
  const protoc = await context.fetchArtifactAlias("protoc:25.4");
  const protocGenGo = await context.fetchArtifactAlias("protoc-gen-go:1.36.3");
  const protocGenGoGrpc = await context.fetchArtifactAlias(
    "protoc-gen-go-grpc:1.70.0",
  );
  const staticcheck = await context.fetchArtifactAlias(
    "staticcheck:2025.1.1",
  );

  const goarch = getGoarch(context.getSystem());
  const goos = getGoos(context.getSystem());

  return new ProjectEnvironmentBuilder("vorpal-shell", SYSTEMS)
    .withArtifacts([
      bun,
      crane,
      go,
      goimports,
      gopls,
      grpcurl,
      nodejs,
      pnpm,
      protoc,
      protocGenGo,
      protocGenGoGrpc,
      staticcheck,
    ])
    .withEnvironments([
      "CGO_ENABLED=0",
      `GOARCH=${goarch}`,
      `GOOS=${goos}`,
    ])
    .build(context);
}

async function buildVorpalUser(context: ConfigContext): Promise<string> {
  return new UserEnvironmentBuilder("vorpal-user", SYSTEMS)
    .withArtifacts([])
    .withEnvironments(["PATH=$HOME/.vorpal/bin"])
    .withSymlinks([
      [
        "$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal",
        "$HOME/.vorpal/bin/vorpal",
      ],
    ])
    .build(context);
}

async function main(): Promise<void> {
  const context = ConfigContext.create();

  switch (context.getArtifactName()) {
    case "vorpal":
      await buildVorpal(context);
      break;
    case "vorpal-container-image":
      await buildVorpalContainerImage(context);
      break;
    case "vorpal-job":
      await buildVorpalJob(context);
      break;
    case "vorpal-process":
      await buildVorpalProcess(context);
      break;
    case "vorpal-shell":
      await buildVorpalShell(context);
      break;
    case "vorpal-user":
      await buildVorpalUser(context);
      break;
    default:
      break;
  }

  await context.run();
}

main();
