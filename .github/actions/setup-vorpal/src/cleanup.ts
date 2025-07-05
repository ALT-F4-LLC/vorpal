import * as core from "@actions/core";
import * as fs from "fs";

export async function cleanup(): Promise<void> {
  try {
    core.info("=== Vorpal Service Cleanup ===");

    // Show final logs
    const logFile = "/tmp/vorpal_output.log";

    if (fs.existsSync(logFile)) {
      const logs = fs.readFileSync(logFile, "utf8");

      core.info("Final service logs:");
      core.info(logs);
    } else {
      core.info("No logs found");
    }

    // Get stored PID and attempt cleanup
    const pidString = core.getState("vorpal-pid");

    if (pidString) {
      const pid = parseInt(pidString, 10);

      if (!isNaN(pid)) {
        core.info(`Attempting to stop Vorpal service (PID: ${pid})`);

        try {
          process.kill(pid, "SIGTERM");
          core.info("Vorpal service stopped");
        } catch (error) {
          const errorMessage =
            error instanceof Error ? error.message : String(error);

          core.info(`Could not stop process: ${errorMessage}`);
        }
      } else {
        core.info("Invalid PID found in state");
      }
    }
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);

    core.error(`Cleanup failed: ${errorMessage}`);
  }
}

cleanup();
