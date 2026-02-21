// Public API re-exports for @vorpal/sdk

// Context
export { ConfigContext, parseArtifactAlias, formatArtifactAlias } from "./context.js";
export type { ArtifactAlias } from "./context.js";

// Builders
export {
  ArtifactBuilder,
  ArtifactSourceBuilder,
  ArtifactStepBuilder,
  Argument,
  JobBuilder,
  ProcessBuilder,
  ProjectEnvironmentBuilder,
  UserEnvironmentBuilder,
  getEnvKey,
} from "./artifact.js";

// Step functions
export { bash, bwrap, shell, docker } from "./artifact/step.js";

// Language builders
export { TypeScriptBuilder } from "./artifact/language/typescript.js";

// System utilities
export {
  getSystem,
  getSystemDefault,
  getSystemDefaultStr,
  getSystemStr,
} from "./system.js";

// CLI
export { parseCliArgs } from "./cli.js";
export type { StartCommand } from "./cli.js";

// Re-export commonly used generated types for convenience
export {
  ArtifactSystem,
} from "./api/artifact/artifact.js";
export type {
  Artifact,
  ArtifactSource,
  ArtifactStep,
  ArtifactStepSecret,
} from "./api/artifact/artifact.js";
