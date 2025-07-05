import { beforeEach, afterEach, describe, expect, it, jest } from '@jest/globals'

let mockCore: any
let mockFs: any
let mockProcess: any

beforeEach(() => {
  jest.resetModules()

  // Mock @actions/core
  mockCore = {
    info: jest.fn(),
    error: jest.fn(),
    getState: jest.fn()
  }

  // Mock fs
  mockFs = {
    existsSync: jest.fn(),
    readFileSync: jest.fn()
  }

  // Mock process.kill
  mockProcess = {
    kill: jest.fn()
  }

  // Setup module mocks
  jest.unstable_mockModule('@actions/core', () => mockCore)
  jest.unstable_mockModule('fs', () => mockFs)
  
  // Mock process.kill globally
  global.process.kill = mockProcess.kill
})

afterEach(() => {
  jest.restoreAllMocks()
})

describe('Cleanup Function', () => {
  describe('cleanup function', () => {
    it('should display logs when log file exists', async () => {
      mockFs.existsSync.mockReturnValue(true)
      mockFs.readFileSync.mockReturnValue('sample log content')
      mockCore.getState.mockReturnValue('') // No PID stored

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.info).toHaveBeenCalledWith('=== Vorpal Service Cleanup ===')
      expect(mockFs.existsSync).toHaveBeenCalledWith('/tmp/vorpal_output.log')
      expect(mockFs.readFileSync).toHaveBeenCalledWith('/tmp/vorpal_output.log', 'utf8')
      expect(mockCore.info).toHaveBeenCalledWith('Final service logs:')
      expect(mockCore.info).toHaveBeenCalledWith('sample log content')
    })

    it('should handle missing log file', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('') // No PID stored

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.info).toHaveBeenCalledWith('=== Vorpal Service Cleanup ===')
      expect(mockFs.existsSync).toHaveBeenCalledWith('/tmp/vorpal_output.log')
      expect(mockFs.readFileSync).not.toHaveBeenCalled()
      expect(mockCore.info).toHaveBeenCalledWith('No logs found')
    })

    it('should stop process when valid PID is stored', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('12345')
      mockProcess.kill.mockImplementation(() => {}) // Successful kill

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.getState).toHaveBeenCalledWith('vorpal-pid')
      expect(mockCore.info).toHaveBeenCalledWith('Attempting to stop Vorpal service (PID: 12345)')
      expect(mockProcess.kill).toHaveBeenCalledWith(12345, 'SIGTERM')
      expect(mockCore.info).toHaveBeenCalledWith('Vorpal service stopped')
    })

    it('should handle process kill failure', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('12345')
      mockProcess.kill.mockImplementation(() => {
        throw new Error('Process not found')
      })

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.getState).toHaveBeenCalledWith('vorpal-pid')
      expect(mockCore.info).toHaveBeenCalledWith('Attempting to stop Vorpal service (PID: 12345)')
      expect(mockProcess.kill).toHaveBeenCalledWith(12345, 'SIGTERM')
      expect(mockCore.info).toHaveBeenCalledWith('Could not stop process: Process not found')
    })

    it('should handle invalid PID string', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('invalid-pid')

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.getState).toHaveBeenCalledWith('vorpal-pid')
      expect(mockCore.info).toHaveBeenCalledWith('Invalid PID found in state')
      expect(mockProcess.kill).not.toHaveBeenCalled()
    })

    it('should handle empty PID state', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('')

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.getState).toHaveBeenCalledWith('vorpal-pid')
      expect(mockProcess.kill).not.toHaveBeenCalled()
      expect(mockCore.info).not.toHaveBeenCalledWith(expect.stringContaining('Attempting to stop'))
    })

    it('should handle non-Error exceptions in process kill', async () => {
      mockFs.existsSync.mockReturnValue(false)
      mockCore.getState.mockReturnValue('12345')
      mockProcess.kill.mockImplementation(() => {
        throw 'String error'
      })

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.info).toHaveBeenCalledWith('Could not stop process: String error')
    })

    it('should handle overall cleanup failure', async () => {
      mockFs.existsSync.mockImplementation(() => {
        throw new Error('Filesystem error')
      })

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.error).toHaveBeenCalledWith('Cleanup failed: Filesystem error')
    })

    it('should handle non-Error cleanup exceptions', async () => {
      mockFs.existsSync.mockImplementation(() => {
        throw 'Filesystem string error'
      })

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      expect(mockCore.error).toHaveBeenCalledWith('Cleanup failed: Filesystem string error')
    })

    it('should complete full cleanup with logs and process termination', async () => {
      mockFs.existsSync.mockReturnValue(true)
      mockFs.readFileSync.mockReturnValue('Service started successfully\\nProcessing requests...')
      mockCore.getState.mockReturnValue('9876')
      mockProcess.kill.mockImplementation(() => {}) // Successful kill

      const { cleanup } = await import('../src/cleanup.js')
      await cleanup()

      // Verify complete flow
      expect(mockCore.info).toHaveBeenCalledWith('=== Vorpal Service Cleanup ===')
      expect(mockCore.info).toHaveBeenCalledWith('Final service logs:')
      expect(mockCore.info).toHaveBeenCalledWith('Service started successfully\\nProcessing requests...')
      expect(mockCore.info).toHaveBeenCalledWith('Attempting to stop Vorpal service (PID: 9876)')
      expect(mockProcess.kill).toHaveBeenCalledWith(9876, 'SIGTERM')
      expect(mockCore.info).toHaveBeenCalledWith('Vorpal service stopped')
    })
  })
})