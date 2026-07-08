import { ArtifactSystem } from "./api/artifact/artifact.js";
import { arch, platform } from "node:os";

export type ArtifactSystemInput = string | ArtifactSystem;

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

function getUnsupportedSystemValue(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.UNKNOWN_SYSTEM:
      return "UNKNOWN_SYSTEM";
    case ArtifactSystem.UNRECOGNIZED:
      return "UNRECOGNIZED";
    default:
      return `${system}`;
  }
}

function normalizeSystem(system: ArtifactSystemInput): ArtifactSystem {
  if (typeof system === "string") {
    return getSystem(system);
  }

  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
    case ArtifactSystem.AARCH64_LINUX:
    case ArtifactSystem.X8664_DARWIN:
    case ArtifactSystem.X8664_LINUX:
      return system;
    default:
      throw new Error(
        `unsupported system: ${getUnsupportedSystemValue(system)}`,
      );
  }
}

/**
 * Normalizes canonical system strings and ArtifactSystem enum values.
 */
export function normalizeSystems(
  systems: ArtifactSystemInput[],
): ArtifactSystem[] {
  return systems.map(normalizeSystem);
}

export function tryNormalizeSystems(
  systems: ArtifactSystemInput[],
): { systems: ArtifactSystem[]; error: Error | undefined } {
  try {
    return { systems: normalizeSystems(systems), error: undefined };
  } catch (error) {
    return {
      systems: [],
      error: error instanceof Error ? error : new Error(String(error)),
    };
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
