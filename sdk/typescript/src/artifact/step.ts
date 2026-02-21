import type {
  ArtifactStep as ArtifactStepMsg,
  ArtifactStepSecret,
} from "../api/artifact/artifact.js";
import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { getEnvKey } from "../artifact.js";

/**
 * Creates a bash step. Matches Rust sdk/rust/src/artifact/step.rs bash().
 *
 * - Filters PATH from environments
 * - Builds PATH from artifact bins + default system paths
 * - Prepends bash shebang and set -euo pipefail to script
 * - Uses "bash" as entrypoint
 */
export function bash(
  artifacts: string[],
  environments: string[],
  secrets: ArtifactStepSecret[],
  script: string,
): ArtifactStepMsg {
  const stepEnvironments: string[] = [];

  for (const environment of environments) {
    if (environment.startsWith("PATH=")) {
      continue;
    }
    stepEnvironments.push(environment);
  }

  const stepPathBins = artifacts
    .map((a) => `${getEnvKey(a)}/bin`)
    .join(":");

  const stepPathDefault = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin";

  let stepPath = `${stepPathBins}:${stepPathDefault}`;

  const pathEnv = environments.find((x) => x.startsWith("PATH="));
  if (pathEnv) {
    const pathValue = pathEnv.split("=").slice(1).join("=");
    if (pathValue) {
      stepPath = `${pathValue}:${stepPath}`;
    }
  }

  stepEnvironments.push("HOME=$VORPAL_WORKSPACE");
  stepEnvironments.push(`PATH=${stepPath}`);

  const stepScript = `#!/bin/bash\nset -euo pipefail\n\n${script}\n`;

  // Deduplicate secrets by name
  const uniqueSecrets: ArtifactStepSecret[] = [];
  for (const secret of secrets) {
    if (!uniqueSecrets.some((s) => s.name === secret.name)) {
      uniqueSecrets.push(secret);
    }
  }

  return {
    entrypoint: "bash",
    script: stepScript,
    secrets: uniqueSecrets,
    arguments: [],
    artifacts: artifacts,
    environments: stepEnvironments,
  };
}

/**
 * Creates a bwrap step. Matches Rust sdk/rust/src/artifact/step.rs bwrap().
 *
 * CRITICAL: Argument list ordering must be identical to Rust/Go.
 */
export function bwrap(
  arguments_: string[],
  artifacts: string[],
  environments: string[],
  rootfs: string | null,
  secrets: ArtifactStepSecret[],
  script: string,
): ArtifactStepMsg {
  // Setup arguments
  const stepArguments: string[] = [
    "--unshare-all",
    "--share-net",
    "--clearenv",
    "--chdir",
    "$VORPAL_WORKSPACE",
    "--gid",
    "1000",
    "--uid",
    "1000",
    "--dev",
    "/dev",
    "--proc",
    "/proc",
    "--tmpfs",
    "/tmp",
    "--bind",
    "$VORPAL_OUTPUT",
    "$VORPAL_OUTPUT",
    "--bind",
    "$VORPAL_WORKSPACE",
    "$VORPAL_WORKSPACE",
    "--setenv",
    "VORPAL_OUTPUT",
    "$VORPAL_OUTPUT",
    "--setenv",
    "VORPAL_WORKSPACE",
    "$VORPAL_WORKSPACE",
    "--setenv",
    "HOME",
    "$VORPAL_WORKSPACE",
  ];

  // Setup artifacts
  const stepArtifacts: string[] = [];

  if (rootfs !== null) {
    const rootfsEnv = getEnvKey(rootfs);
    const rootfsBin = `${rootfsEnv}/bin`;
    const rootfsEtc = `${rootfsEnv}/etc`;
    const rootfsLib = `${rootfsEnv}/lib`;
    const rootfsLib64 = `${rootfsEnv}/lib64`;
    const rootfsSbin = `${rootfsEnv}/sbin`;
    const rootfsUsr = `${rootfsEnv}/usr`;

    const rootfsArgs = [
      "--ro-bind",
      rootfsBin,
      "/bin",
      "--ro-bind",
      rootfsEtc,
      "/etc",
      "--ro-bind",
      rootfsLib,
      "/lib",
      "--ro-bind-try",
      rootfsLib64,
      "/lib64",
      "--ro-bind",
      rootfsSbin,
      "/sbin",
      "--ro-bind",
      rootfsUsr,
      "/usr",
    ];

    stepArguments.push(...rootfsArgs);
    stepArtifacts.push(rootfs);
  }

  // Setup artifact arguments
  for (const artifact of artifacts) {
    stepArtifacts.push(artifact);
  }

  for (const artifact of stepArtifacts) {
    stepArguments.push("--ro-bind");
    stepArguments.push(getEnvKey(artifact));
    stepArguments.push(getEnvKey(artifact));
    stepArguments.push("--setenv");
    stepArguments.push(getEnvKey(artifact).replace("$", ""));
    stepArguments.push(getEnvKey(artifact));
  }

  // Setup environment arguments
  const stepPathBins = stepArtifacts
    .map((a) => `${getEnvKey(a)}/bin`)
    .join(":");

  let stepPath = `${stepPathBins}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin`;

  const pathEnv = environments.find((x) => x.startsWith("PATH="));
  if (pathEnv) {
    const pathValue = pathEnv.split("=").slice(1).join("=");
    if (pathValue) {
      stepPath = `${pathValue}:${stepPath}`;
    }
  }

  stepArguments.push("--setenv");
  stepArguments.push("PATH");
  stepArguments.push(stepPath);

  for (const env of environments) {
    const eqIdx = env.indexOf("=");
    const key = eqIdx !== -1 ? env.substring(0, eqIdx) : env;
    const value = eqIdx !== -1 ? env.substring(eqIdx + 1) : "";

    if (key.startsWith("PATH")) {
      continue;
    }

    stepArguments.push("--setenv");
    stepArguments.push(key);
    stepArguments.push(value);
  }

  // Setup arguments
  for (const argument of arguments_) {
    stepArguments.push(argument);
  }

  // Setup script
  const stepScript = `#!/bin/bash\nset -euo pipefail\n\n${script}\n`;

  // Deduplicate secrets by name
  const uniqueSecrets: ArtifactStepSecret[] = [];
  for (const secret of secrets) {
    if (!uniqueSecrets.some((s) => s.name === secret.name)) {
      uniqueSecrets.push(secret);
    }
  }

  // Setup step
  return {
    entrypoint: "bwrap",
    script: stepScript,
    secrets: uniqueSecrets,
    arguments: stepArguments,
    artifacts: stepArtifacts,
    environments: ["PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"],
  };
}

/**
 * Creates a shell step. Matches Rust sdk/rust/src/artifact/step.rs shell().
 *
 * Dispatches to bash() on Darwin, bwrap() on Linux with linux-vorpal rootfs.
 */
export async function shell(
  context: ConfigContext,
  artifacts: string[],
  environments: string[],
  script: string,
  secrets: ArtifactStepSecret[],
): Promise<ArtifactStepMsg> {
  const stepSystem = context.getSystem();

  if (
    stepSystem === ArtifactSystem.AARCH64_DARWIN ||
    stepSystem === ArtifactSystem.X8664_DARWIN
  ) {
    return bash(artifacts, environments, secrets, script);
  }

  if (
    stepSystem === ArtifactSystem.AARCH64_LINUX ||
    stepSystem === ArtifactSystem.X8664_LINUX
  ) {
    const linuxVorpal = await context.fetchArtifactAlias(
      "library/linux-vorpal:latest",
    );

    return bwrap([], artifacts, environments, linuxVorpal, secrets, script);
  }

  throw new Error(`unsupported system: ${stepSystem}`);
}

/**
 * Creates a docker step. Matches Rust sdk/rust/src/artifact/step.rs docker().
 */
export function docker(
  arguments_: string[],
  artifacts: string[],
): ArtifactStepMsg {
  return {
    entrypoint: "docker",
    script: undefined,
    secrets: [],
    arguments: arguments_,
    artifacts: artifacts,
    environments: ["PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"],
  };
}
