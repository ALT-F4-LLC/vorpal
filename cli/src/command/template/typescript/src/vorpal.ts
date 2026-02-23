import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
  TypeScriptDevelopmentEnvironment,
} from "@vorpal/sdk";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

// Development environment

await new TypeScriptDevelopmentEnvironment("example-shell", SYSTEMS)
  .build(context);

// Artifacts

await new TypeScript("example", SYSTEMS)
  .withIncludes(["src", "package.json", "tsconfig.json", "bun.lock"])
  .build(context);

// Run the build

await context.run();
