import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
} from "@vorpal/sdk";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

// Artifacts

await new TypeScript("example", SYSTEMS)
  .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
  .build(context);

// Run the build

await context.run();
