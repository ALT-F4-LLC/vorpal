import { ArtifactSystem } from "./api/artifact/artifact.js";
import { RustBuilder } from "./artifact/language/rust.js";
import { ConfigContext } from "./context.js";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

async function main(): Promise<void> {
  const context = ConfigContext.create();

  switch (context.getArtifactName()) {
    case "vorpal":
      await new RustBuilder("vorpal", SYSTEMS)
        .withBins(["vorpal"])
        .withIncludes(["cli", "sdk/rust"])
        .withPackages(["vorpal-cli", "vorpal-sdk"])
        .build(context);
      break;
    default:
      break;
  }

  await context.run();
}

main();
