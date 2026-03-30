import { ArtifactSystem } from "./api/artifact/artifact.js";
import {
  getEnvKey,
  Job,
  OciImage,
  Process,
  DevelopmentEnvironment,
  UserEnvironment,
} from "./artifact.js";
import { Bun } from "./artifact/bun.js";
import { Crane } from "./artifact/crane.js";
import { GoBin } from "./artifact/go.js";
import { Goimports } from "./artifact/goimports.js";
import { Gopls } from "./artifact/gopls.js";
import { Grpcurl } from "./artifact/grpcurl.js";
import { Rust } from "./artifact/language/rust.js";
import { NodeJS } from "./artifact/nodejs.js";
import { Pnpm } from "./artifact/pnpm.js";
import { Protoc } from "./artifact/protoc.js";
import { ProtocGenGo } from "./artifact/protoc_gen_go.js";
import { ProtocGenGoGrpc } from "./artifact/protoc_gen_go_grpc.js";
import { Staticcheck } from "./artifact/staticcheck.js";
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
  return new Rust("vorpal", SYSTEMS)
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

  return new OciImage(name, linuxVorpalSlim)
    .withAliases([`${name}:latest`])
    .withArtifacts([vorpal])
    .build(context);
}

async function buildVorpalJob(context: ConfigContext): Promise<string> {
  const vorpal = await buildVorpal(context);
  const script = `${getEnvKey(vorpal)}/bin/vorpal --version`;

  return new Job("vorpal-job", script, SYSTEMS)
    .withArtifacts([vorpal])
    .build(context);
}

async function buildVorpalProcess(context: ConfigContext): Promise<string> {
  const vorpal = await buildVorpal(context);

  return new Process(
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
  const bun = await new Bun().build(context);
  const crane = await new Crane().build(context);
  const go = await new GoBin().build(context);
  const goimports = await new Goimports().build(context);
  const gopls = await new Gopls().build(context);
  const grpcurl = await new Grpcurl().build(context);
  const nodejs = await new NodeJS().build(context);
  const pnpm = await new Pnpm().build(context);
  const protoc = await new Protoc().build(context);
  const protocGenGo = await new ProtocGenGo().build(context);
  const protocGenGoGrpc = await new ProtocGenGoGrpc().build(context);
  const staticcheck = await new Staticcheck().build(context);

  const goarch = getGoarch(context.getSystem());
  const goos = getGoos(context.getSystem());

  return new DevelopmentEnvironment("vorpal-shell", SYSTEMS)
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
  return new UserEnvironment("vorpal-user", SYSTEMS)
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
