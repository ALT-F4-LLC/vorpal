import { ArtifactSystem } from "./api/artifact/artifact.js";
import { getEnvKey, JobBuilder, ProcessBuilder } from "./artifact.js";
import { RustBuilder } from "./artifact/language/rust.js";
import { ConfigContext } from "./context.js";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

async function buildVorpal(context: ConfigContext): Promise<string> {
  return new RustBuilder("vorpal", SYSTEMS)
    .withBins(["vorpal"])
    .withIncludes(["cli", "sdk/rust"])
    .withPackages(["vorpal-cli", "vorpal-sdk"])
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

async function main(): Promise<void> {
  const context = ConfigContext.create();

  switch (context.getArtifactName()) {
    case "vorpal":
      await buildVorpal(context);
      break;
    case "vorpal-job":
      await buildVorpalJob(context);
      break;
    case "vorpal-process":
      await buildVorpalProcess(context);
      break;
    default:
      break;
  }

  await context.run();
}

main();
