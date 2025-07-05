const core = require('@actions/core');
const fs = require('fs');

async function cleanup() {
    try {
        core.info('=== Vorpal Service Cleanup ===');

        // Show final logs
        const logFile = '/tmp/vorpal_output.log';

        if (fs.existsSync(logFile)) {
            const logs = fs.readFileSync(logFile, 'utf8');

            core.info('Final service logs:');
            core.info(logs);
        } else {
            core.info('No logs found');
        }

        // Get stored PID and attempt cleanup
        const pid = core.getState('vorpal-pid');

        if (pid) {
            core.info(`Attempting to stop Vorpal service (PID: ${pid})`);

            try {
                process.kill(pid, 'SIGTERM');
                core.info('Vorpal service stopped');
            } catch (error) {
                core.info(`Could not stop process: ${error.message}`);
            }
        }

    } catch (error) {
        core.error(`Cleanup failed: ${error.message}`);
    }
}

cleanup();
