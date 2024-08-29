import * as core from '@actions/core'
import * as exec from '@actions/exec'
import * as fs from 'fs'
import * as os from 'os'
import * as toolcache from '@actions/tool-cache'

const DEFAULT_VERSION = 'edge'
const SUPPORTED_SYSTEMS = ['x86_64-linux']

/**
 * Ensure the directory exists, create it if it doesn't.
 * @param dirPath The path of the directory to check/create.
 */
function ensureDirectoryExists(dirPath: string): void {
    const uid = os.userInfo().uid
    const gid = os.userInfo().gid
    if (!fs.existsSync(dirPath)) {
        exec.exec(`sudo`, ['mkdir', '-p', '/var/lib/vorpal/bin'], {})
        exec.exec(`sudo`, ['chown', '-R', `${uid}:${gid}`, '/var/lib/vorpal'], {})
        core.info(`Vorpal dir created: ${dirPath}`)
    }
}

/**
 * The main function for the action.
 * @returns {Promise<void>} Resolves when the action is complete.
 */
export async function run(): Promise<void> {
    try {
        const platform = os.platform()
        const arch = os.arch() === 'x64' ? 'x86_64' : os.arch()
        const system = `${arch}-${platform}`

        core.info(`Current system: ${system}`)
        core.info(`Supported systems: ${SUPPORTED_SYSTEMS.join(', ')}`)
        core.info(`Version: ${DEFAULT_VERSION}`)

        if (!SUPPORTED_SYSTEMS.includes(system)) {
            core.setFailed(`System ${system} is not supported.`)
            return
        }

        const vorpalDir = '/var/lib/vorpal'

        ensureDirectoryExists(vorpalDir)

        const downloadUrl = `https://github.com/ALT-F4-LLC/vorpal/releases/download/${DEFAULT_VERSION}/vorpal-${system}.tar.gz`
        const packagePath = await toolcache.downloadTool(downloadUrl)
        const binPath = await toolcache.extractTar(packagePath, `${vorpalDir}/bin`)

        core.addPath(binPath)

        let generateOutput = ''
        let generateError = ''

        const generateOptions: exec.ExecOptions = {
            listeners: {
                stdout: (data: Buffer) => (generateOutput += data.toString()),
                stderr: (data: Buffer) => (generateError += data.toString())
            }
        }

        exec.exec(`${binPath}/vorpal`, ['keys', 'generate'], generateOptions)

        if (generateError !== '') {
            core.setFailed(`Generate failed: ${generateError}`)
            return
        }
    } catch (error) {
        if (error instanceof Error) core.setFailed(error.message)
    }
}
