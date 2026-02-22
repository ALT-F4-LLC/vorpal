import {
  ArtifactSystem,
  ConfigContext,
  JobBuilder,
} from "@vorpal/sdk";

const SYSTEMS: ArtifactSystem[] = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

// Artifacts

await new JobBuilder(
  "example",
  'echo "Hello from example!"',
  SYSTEMS,
).build(context);

// Run the build

await context.run();
