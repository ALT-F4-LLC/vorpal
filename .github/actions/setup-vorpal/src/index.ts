import * as core from "@actions/core";
import * as exec from "@actions/exec";
import { spawn, ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";

interface VorpalInputs {
  port: string;
  registryBackend: string;
  registryBackendS3Bucket: string;
  services: string;
  useLocalBuild: boolean;
  version: string;
}

export async function run(): Promise<void> {
  try {
    // Get inputs
    const inputs: VorpalInputs = {
      version: core.getInput("version"),
      useLocalBuild: core.getInput("use-local-build") === "true",
      registryBackend: core.getInput("registry-backend") || "local",
      registryBackendS3Bucket: core.getInput("registry-backend-s3-bucket"),
      port: core.getInput("port") || "23151",
      services: core.getInput("services") || "agent,registry,worker",
    };

    // Step 1: Install Vorpal
    await installVorpal(inputs.version, inputs.useLocalBuild);

    // Step 2: Setup Vorpal Directories
    await setupVorpalDirectories();

    // Step 3: Generate Vorpal Keys
    await generateVorpalKeys();

    // Step 4: Start Vorpal
    await startVorpal(
      inputs.registryBackend,
      inputs.registryBackendS3Bucket,
      inputs.port,
      inputs.services,
    );
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    core.setFailed(errorMessage);
  }
}

export async function installVorpal(
  version: string,
  useLocalBuild: boolean,
): Promise<void> {
  core.info("Installing Vorpal...");

  if (useLocalBuild) {
    core.info("Using local build of vorpal");
    await exec.exec("chmod", ["+x", "./dist/vorpal"]);
    core.addPath(path.join(process.cwd(), "dist"));
  } else {
    if (!version) {
      throw new Error(
        "'version' input is required when 'use-local-build' is false.",
      );
    }

    const os = process.platform === "darwin" ? "darwin" : "linux";
    const arch = process.arch === "x64" ? "x86_64" : "aarch64";
    const releaseAsset = `vorpal-${arch}-${os}.tar.gz`;
    const releaseUrl = `https://github.com/ALT-F4-LLC/vorpal/releases/download/${version}/${releaseAsset}`;

    core.info(`Downloading from ${releaseUrl}`);

    await exec.exec("curl", ["-sSL", "-o", releaseAsset, releaseUrl]);
    await exec.exec("tar", ["-xzf", releaseAsset]);
    await exec.exec("rm", [releaseAsset]);
    await exec.exec("chmod", ["+x", "vorpal"]);

    core.addPath(process.cwd());
  }
}

export async function setupVorpalDirectories(): Promise<void> {
  core.info("Setting up Vorpal directories...");

  // Create directories using a loop since brace expansion doesn't work with exec.exec
  const directories: string[] = [
    "/var/lib/vorpal/key",
    "/var/lib/vorpal/sandbox",
    "/var/lib/vorpal/store",
    "/var/lib/vorpal/store/artifact/alias",
    "/var/lib/vorpal/store/artifact/archive",
    "/var/lib/vorpal/store/artifact/config",
    "/var/lib/vorpal/store/artifact/output",
  ];

  for (const dir of directories) {
    await exec.exec("sudo", ["mkdir", "-pv", dir]);
  }

  // Get current user and group dynamically
  if (!process.getuid || !process.getgid) {
    throw new Error(
      "Unable to get user/group ID - not supported on this platform",
    );
  }

  const uid = process.getuid();
  const gid = process.getgid();

  core.info(`Setting ownership to ${uid}:${gid}`);
  await exec.exec("sudo", ["chown", "-R", `${uid}:${gid}`, "/var/lib/vorpal"]);
}

export async function generateVorpalKeys(): Promise<void> {
  core.info("Generating Vorpal keys...");
  await exec.exec("vorpal", ["system", "keys", "generate"]);
}

export async function startVorpal(
  registryBackend: string,
  registryBackendS3Bucket: string,
  port: string,
  services: string,
): Promise<void> {
  core.info("Starting Vorpal service...");

  // Build command arguments
  const args: string[] = [
    "services",
    "start",
    "--port",
    port,
    "--services",
    services,
    "--registry-backend",
    registryBackend,
  ];

  // Add S3 bucket if S3 backend is specified
  if (registryBackend === "s3") {
    if (!registryBackendS3Bucket) {
      throw new Error(
        "registry-backend-s3-bucket is required when using s3 backend",
      );
    }

    // Check for required AWS environment variables
    const awsAccessKeyId = process.env.AWS_ACCESS_KEY_ID;
    const awsDefaultRegion = process.env.AWS_DEFAULT_REGION;
    const awsSecretAccessKey = process.env.AWS_SECRET_ACCESS_KEY;

    if (!awsAccessKeyId) {
      throw new Error(
        "AWS_ACCESS_KEY_ID environment variable is required when using s3 backend",
      );
    }
    if (!awsDefaultRegion) {
      throw new Error(
        "AWS_DEFAULT_REGION environment variable is required when using s3 backend",
      );
    }
    if (!awsSecretAccessKey) {
      throw new Error(
        "AWS_SECRET_ACCESS_KEY environment variable is required when using s3 backend",
      );
    }

    core.info("AWS credentials validated for S3 backend");
    args.push("--registry-backend-s3-bucket", registryBackendS3Bucket);
  }

  const command = `vorpal ${args.join(" ")}`;
  core.info(`Starting vorpal with command: ${command}`);

  // Start the service in background
  const logFile = "/tmp/vorpal_output.log";
  const logFd = fs.openSync(logFile, "w");

  // Prepare environment variables for the process
  const env = { ...process.env };

  // Ensure AWS credentials are passed to the process when using S3 backend
  if (registryBackend === "s3") {
    env.AWS_ACCESS_KEY_ID = process.env.AWS_ACCESS_KEY_ID;
    env.AWS_DEFAULT_REGION = process.env.AWS_DEFAULT_REGION;
    env.AWS_SECRET_ACCESS_KEY = process.env.AWS_SECRET_ACCESS_KEY;
  }

  const child: ChildProcess = spawn("vorpal", args, {
    stdio: ["ignore", logFd, logFd],
    detached: true,
    env: env,
  });

  // Detach the process from the parent
  child.unref();

  // Close our reference to the file descriptor
  fs.closeSync(logFd);

  // Give it a moment to start
  await new Promise((resolve) => setTimeout(resolve, 2000));

  // Check if process is still running
  if (child.killed || child.exitCode !== null) {
    const logs = fs.readFileSync(logFile, "utf8");
    core.error("Vorpal service failed to start");
    core.error("Service output:");
    core.error(logs);
    throw new Error("Vorpal service failed to start");
  }

  core.info(`Vorpal service is running (PID: ${child.pid})`);

  // Store PID for cleanup
  if (child.pid) {
    core.saveState("vorpal-pid", child.pid.toString());
  }

  // Show initial logs
  await new Promise((resolve) => setTimeout(resolve, 500)); // Wait for file to be written

  if (fs.existsSync(logFile)) {
    const logs = fs.readFileSync(logFile, "utf8");

    core.info("Initial service logs:");
    core.info(logs);
  }
}

run();
