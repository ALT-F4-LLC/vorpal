import * as core from '@actions/core'
import * as toolcache from '@actions/tool-cache'
import * as os from 'os'
import * as fs from 'fs'

const DEFAULT_VERSION = '0.1.0-rc.0'

const SUPPORTED_SYSTEMS = ['x64-linux']

/**
 * The main function for the action.
 * @returns {Promise<void>} Resolves when the action is complete.
 */
export async function run(): Promise<void> {
  try {
    let version = core.getInput('version')

    if (version === '') version = DEFAULT_VERSION

    const baseUrl = 'https://github.com/ALT-F4-LLC/vorpal/releases/download'

    let system = `${os.arch()}-${os.platform()}`

    if (!SUPPORTED_SYSTEMS.includes(system)) {
      throw new Error(`Unsupported system: ${system}`)
    }

    switch (system) {
      case 'x64-linux':
        system = 'x86_64-linux'
        break
    }

    const archivePath = await toolcache.downloadTool(
      `${baseUrl}/${version}/vorpal-${system}.tar.gz`
    )

    const vorpalBinPath = '/tmp/vorpal/bin'

    fs.mkdirSync(vorpalBinPath, { recursive: true })

    const binaryPath = await toolcache.extractTar(archivePath, vorpalBinPath)

    core.info(`Extracted binary to ${binaryPath}`)

    core.addPath(vorpalBinPath)
  } catch (error) {
    // Fail the workflow run if an error occurs
    if (error instanceof Error) core.setFailed(error.message)
  }
}
