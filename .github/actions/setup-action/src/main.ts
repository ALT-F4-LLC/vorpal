import * as core from '@actions/core'

const DEFAULT_VERSION = '0.1.0-rc.0'

const SUPPORTED_SYSTEMS = ['x64-linux']

/**
 * The main function for the action.
 * @returns {Promise<void>} Resolves when the action is complete.
 */
export async function run(): Promise<void> {
    try {
        // Import the exec module from the actions toolkit

        // Generate a worker container run command

        // Run the worker container
    } catch (error) {
        // Fail the workflow run if an error occurs
        if (error instanceof Error) core.setFailed(error.message)
    }
}
