import * as core from '@actions/core'
import * as toolcache from '@actions/tool-cache'
import * as os from 'os'

const DEFAULT_VERSION = 'edge'
const SUPPORTED_SYSTEMS = ['x86_64-linux']

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
      throw new Error(`System ${system} is not supported.`)
    }

    const downloadUrl = `https://github.com/ALT-F4-LLC/vorpal/releases/download/${DEFAULT_VERSION}/vorpal-${system}.tar.gz`
    const packagePath = await toolcache.downloadTool(downloadUrl)
    const binPath = await toolcache.extractTar(packagePath, '/tmp/vorpal/bin')

    core.addPath(binPath)
  } catch (error) {
    if (error instanceof Error) core.setFailed(error.message)
  }
}
