// Public API re-exports for @altf4llc/vorpal-sdk

// Context
export { ConfigContext, parseArtifactAlias, formatArtifactAlias } from "./context.js";
export type { ArtifactAlias } from "./context.js";

// Builders
export {
  Artifact,
  ArtifactSource,
  ArtifactStep,
  Argument,
  Job,
  OciImage,
  Process,
  DevelopmentEnvironment,
  UserEnvironment,
  getEnvKey,
  secretsToProto,
} from "./artifact.js";

// Step functions
export { bash, bwrap, shell, docker } from "./artifact/step.js";

// Language builders
export { Go } from "./artifact/language/go.js";
export { Rust } from "./artifact/language/rust.js";
export { TypeScript } from "./artifact/language/typescript.js";

// Development environment builders
export { GoDevelopmentEnvironment } from "./artifact/language/go.js";
export { RustDevelopmentEnvironment } from "./artifact/language/rust.js";
export { TypeScriptDevelopmentEnvironment } from "./artifact/language/typescript.js";

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
  ArtifactStepSecret,
} from "./api/artifact/artifact.js";
