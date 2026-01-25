import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// Mock API modules
vi.mock('@/api/pty', () => ({
  getPtyStatus: vi.fn(),
  saveTerminalHistory: vi.fn(),
  saveAllTerminalHistory: vi.fn(),
}))

// Mock useTerminalSessions
const mockRunningSessions = { value: [] as TerminalSession[] }
vi.mock('../useTerminalSessions', () => ({
  useTerminalSessions: vi.fn(() => ({
    runningSessions: mockRunningSessions,
  })),
}))

// Mock session data
const createMockSession = (overrides: Partial<TerminalSession> = {}): TerminalSession => ({
  id: 'terminal-1',
  name: 'Terminal 1',
  status: 'running',
  created_at: 1000,
  updated_at: 2000,
  history: null,
  stopped_at: null,
  ...overrides,
})

describe('useTerminalAutoSave', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
    mockRunningSessions.value = []
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.resetModules()
  })

  describe('saveAllRunningTerminals', () => {
    it('should save history for all running terminals with active PTY', async () => {
      const session1 = createMockSession({ id: 'session-1' })
      const session2 = createMockSession({ id: 'session-2' })
      mockRunningSessions.value = [session1, session2]

      const ptyApi = await import('@/api/pty')
      vi.mocked(ptyApi.getPtyStatus).mockResolvedValue(true)
      vi.mocked(ptyApi.saveTerminalHistory).mockResolvedValue(undefined)

      // Import and get the function
      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { saveAllRunningTerminals } = useTerminalAutoSave()

      await saveAllRunningTerminals()

      expect(ptyApi.getPtyStatus).toHaveBeenCalledWith('session-1')
      expect(ptyApi.getPtyStatus).toHaveBeenCalledWith('session-2')
      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledWith('session-1')
      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledWith('session-2')
    })

    it('should skip saving for terminals with inactive PTY', async () => {
      const session1 = createMockSession({ id: 'session-1' })
      const session2 = createMockSession({ id: 'session-2' })
      mockRunningSessions.value = [session1, session2]

      const ptyApi = await import('@/api/pty')
      // Only session-1 has active PTY
      vi.mocked(ptyApi.getPtyStatus).mockImplementation(async (id) => {
        return id === 'session-1'
      })
      vi.mocked(ptyApi.saveTerminalHistory).mockResolvedValue(undefined)

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { saveAllRunningTerminals } = useTerminalAutoSave()

      await saveAllRunningTerminals()

      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledWith('session-1')
      expect(ptyApi.saveTerminalHistory).not.toHaveBeenCalledWith('session-2')
    })

    it('should handle errors gracefully and continue with other sessions', async () => {
      const session1 = createMockSession({ id: 'session-1' })
      const session2 = createMockSession({ id: 'session-2' })
      mockRunningSessions.value = [session1, session2]

      const ptyApi = await import('@/api/pty')
      vi.mocked(ptyApi.getPtyStatus).mockResolvedValue(true)
      // First session fails, second succeeds
      vi.mocked(ptyApi.saveTerminalHistory).mockImplementation(async (id) => {
        if (id === 'session-1') {
          throw new Error('Save failed')
        }
      })

      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { saveAllRunningTerminals } = useTerminalAutoSave()

      // Should not throw
      await saveAllRunningTerminals()

      // Should still try to save session-2
      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledWith('session-2')
      expect(consoleSpy).toHaveBeenCalled()

      consoleSpy.mockRestore()
    })

    it('should do nothing when no running sessions', async () => {
      mockRunningSessions.value = []

      const ptyApi = await import('@/api/pty')

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { saveAllRunningTerminals } = useTerminalAutoSave()

      await saveAllRunningTerminals()

      expect(ptyApi.getPtyStatus).not.toHaveBeenCalled()
      expect(ptyApi.saveTerminalHistory).not.toHaveBeenCalled()
    })
  })

  describe('startAutoSave / stopAutoSave', () => {
    it('should start interval timer', async () => {
      mockRunningSessions.value = []

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { startAutoSave, stopAutoSave } = useTerminalAutoSave()

      const consoleSpy = vi.spyOn(console, 'debug').mockImplementation(() => {})

      startAutoSave()

      expect(consoleSpy).toHaveBeenCalledWith('Terminal auto-save started')

      // Cleanup
      stopAutoSave()
      consoleSpy.mockRestore()
    })

    it('should not start multiple intervals', async () => {
      mockRunningSessions.value = []

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { startAutoSave, stopAutoSave } = useTerminalAutoSave()

      const consoleSpy = vi.spyOn(console, 'debug').mockImplementation(() => {})

      startAutoSave()
      startAutoSave() // Second call should be ignored

      // Should only log once
      expect(consoleSpy.mock.calls.filter((c) => c[0] === 'Terminal auto-save started')).toHaveLength(1)

      // Cleanup
      stopAutoSave()
      consoleSpy.mockRestore()
    })

    it('should stop interval timer', async () => {
      mockRunningSessions.value = []

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { startAutoSave, stopAutoSave } = useTerminalAutoSave()

      const consoleSpy = vi.spyOn(console, 'debug').mockImplementation(() => {})

      startAutoSave()
      stopAutoSave()

      expect(consoleSpy).toHaveBeenCalledWith('Terminal auto-save stopped')

      consoleSpy.mockRestore()
    })
  })

  describe('interval behavior', () => {
    it('should save terminals at 30 second intervals', async () => {
      const session = createMockSession({ id: 'session-1' })
      mockRunningSessions.value = [session]

      const ptyApi = await import('@/api/pty')
      vi.mocked(ptyApi.getPtyStatus).mockResolvedValue(true)
      vi.mocked(ptyApi.saveTerminalHistory).mockResolvedValue(undefined)

      const { useTerminalAutoSave } = await import('../useTerminalAutoSave')
      const { startAutoSave, stopAutoSave } = useTerminalAutoSave()

      const consoleSpy = vi.spyOn(console, 'debug').mockImplementation(() => {})

      startAutoSave()

      // Initially not called
      expect(ptyApi.saveTerminalHistory).not.toHaveBeenCalled()

      // Advance by 30 seconds
      await vi.advanceTimersByTimeAsync(30000)

      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledWith('session-1')

      // Advance by another 30 seconds
      await vi.advanceTimersByTimeAsync(30000)

      expect(ptyApi.saveTerminalHistory).toHaveBeenCalledTimes(2)

      // Cleanup
      stopAutoSave()
      consoleSpy.mockRestore()
    })
  })
})
