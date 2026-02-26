import { ArtifactSystem } from "./api/artifact/artifact.js";
import { arch, platform } from "node:os";

/**
 * Returns the default system string for the current platform.
 * Format: "{arch}-{os}" (e.g., "aarch64-darwin", "x86_64-linux")
 */
export function getSystemDefaultStr(): string {
  const nodeArch = arch();
  const nodePlatform = platform();

  let cpuArch: string;
  switch (nodeArch) {
    case "arm64":
      cpuArch = "aarch64";
      break;
    case "x64":
      cpuArch = "x86_64";
      break;
    default:
      cpuArch = nodeArch;
  }

  let os: string;
  switch (nodePlatform) {
    case "darwin":
      os = "darwin";
      break;
    case "linux":
      os = "linux";
      break;
    default:
      os = nodePlatform;
  }

  return `${cpuArch}-${os}`;
}

/**
 * Returns the ArtifactSystem enum value for the current platform.
 */
export function getSystemDefault(): ArtifactSystem {
  const platformStr = getSystemDefaultStr();
  return getSystem(platformStr);
}

/**
 * Converts a system string to an ArtifactSystem enum value.
 * Throws if the system string is not recognized.
 */
export function getSystem(system: string): ArtifactSystem {
  switch (system) {
    case "aarch64-darwin":
      return ArtifactSystem.AARCH64_DARWIN;
    case "aarch64-linux":
      return ArtifactSystem.AARCH64_LINUX;
    case "x86_64-darwin":
      return ArtifactSystem.X8664_DARWIN;
    case "x86_64-linux":
      return ArtifactSystem.X8664_LINUX;
    default:
      throw new Error(`unsupported system: ${system}`);
  }
}

/**
 * Converts an ArtifactSystem enum value to a system string.
 */
export function getSystemStr(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
      return "aarch64-darwin";
    case ArtifactSystem.AARCH64_LINUX:
      return "aarch64-linux";
    case ArtifactSystem.X8664_DARWIN:
      return "x86_64-darwin";
    case ArtifactSystem.X8664_LINUX:
      return "x86_64-linux";
    default:
      return "unknown";
  }
}
