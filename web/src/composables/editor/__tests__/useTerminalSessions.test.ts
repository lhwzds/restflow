import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// Mock API modules
vi.mock('@/api/terminal-sessions', () => ({
  listTerminalSessions: vi.fn(),
  createTerminalSession: vi.fn(),
  deleteTerminalSession: vi.fn(),
  renameTerminalSession: vi.fn(),
}))

vi.mock('@/api/pty', () => ({
  restartTerminal: vi.fn(),
}))

// Mock session data
const createMockSession = (overrides: Partial<TerminalSession> = {}): TerminalSession => ({
  id: 'terminal-1',
  name: 'Terminal 1',
  status: 'running',
  created_at: 1000,
  history: null,
  stopped_at: null,
  working_directory: null,
  startup_command: null,
  ...overrides,
})

describe('useTerminalSessions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with empty sessions array', async () => {
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([])

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, isLoading } = useTerminalSessions()

      // Wait for initial load
      await new Promise((resolve) => setTimeout(resolve, 10))

      expect(sessions.value).toEqual([])
      expect(isLoading.value).toBe(false)
    })
  })

  describe('loadSessions', () => {
    it('should load sessions from API', async () => {
      const mockSessions = [
        createMockSession({ id: 'session-1' }),
        createMockSession({ id: 'session-2' }),
      ]
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue(mockSessions)

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      expect(terminalApi.listTerminalSessions).toHaveBeenCalled()
      expect(sessions.value).toEqual(mockSessions)
    })

    it('should handle API errors gracefully', async () => {
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockRejectedValue(new Error('Network error'))

      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {})

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      expect(sessions.value).toEqual([])
      expect(consoleSpy).toHaveBeenCalled()

      consoleSpy.mockRestore()
    })
  })

  describe('createSession', () => {
    it('should create session and add to local state', async () => {
      const newSession = createMockSession({ id: 'new-session' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([])
      vi.mocked(terminalApi.createTerminalSession).mockResolvedValue(newSession)

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, createSession } = useTerminalSessions()

      // Wait for initial load
      await new Promise((resolve) => setTimeout(resolve, 10))

      const result = await createSession()

      expect(terminalApi.createTerminalSession).toHaveBeenCalled()
      expect(result).toEqual(newSession)
      expect(sessions.value).toContainEqual(newSession)
    })

    it('should throw error on failure', async () => {
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([])
      vi.mocked(terminalApi.createTerminalSession).mockRejectedValue(new Error('Create failed'))

      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {})

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { createSession } = useTerminalSessions()

      await new Promise((resolve) => setTimeout(resolve, 10))

      await expect(createSession()).rejects.toThrow('Create failed')

      consoleSpy.mockRestore()
    })
  })

  describe('deleteSession', () => {
    it('should delete session and remove from local state', async () => {
      const existingSession = createMockSession({ id: 'session-to-delete' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([existingSession])
      vi.mocked(terminalApi.deleteTerminalSession).mockResolvedValue(undefined)

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, deleteSession, refreshSessions } = useTerminalSessions()

      await refreshSessions()
      expect(sessions.value).toHaveLength(1)

      await deleteSession('session-to-delete')

      expect(terminalApi.deleteTerminalSession).toHaveBeenCalledWith('session-to-delete')
      expect(sessions.value).toHaveLength(0)
    })
  })

  describe('renameSession', () => {
    it('should rename session and update local state', async () => {
      const existingSession = createMockSession({ id: 'session-1', name: 'Old Name' })
      const updatedSession = { ...existingSession, name: 'New Name' }
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([existingSession])
      vi.mocked(terminalApi.renameTerminalSession).mockResolvedValue(updatedSession)

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, renameSession, refreshSessions } = useTerminalSessions()

      await refreshSessions()
      const result = await renameSession('session-1', 'New Name')

      expect(terminalApi.renameTerminalSession).toHaveBeenCalledWith('session-1', 'New Name')
      expect(result!.name).toBe('New Name')
      expect(sessions.value[0]!.name).toBe('New Name')
    })
  })

  describe('restartSession', () => {
    it('should restart session and update local state', async () => {
      const stoppedSession = createMockSession({ id: 'session-1', status: 'stopped' })
      const restartedSession = { ...stoppedSession, status: 'running' as const }
      const terminalApi = await import('@/api/terminal-sessions')
      const ptyApi = await import('@/api/pty')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([stoppedSession])
      vi.mocked(ptyApi.restartTerminal).mockResolvedValue(restartedSession)

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, restartSession, refreshSessions } = useTerminalSessions()

      await refreshSessions()
      const result = await restartSession('session-1')

      expect(ptyApi.restartTerminal).toHaveBeenCalledWith('session-1')
      expect(result!.status).toBe('running')
      expect(sessions.value[0]!.status).toBe('running')
    })
  })

  describe('getSession', () => {
    it('should return session by id from local state', async () => {
      const session1 = createMockSession({ id: 'session-1' })
      const session2 = createMockSession({ id: 'session-2' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([session1, session2])

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { getSession, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      expect(getSession('session-1')).toEqual(session1)
      expect(getSession('session-2')).toEqual(session2)
      expect(getSession('nonexistent')).toBeUndefined()
    })
  })

  describe('computed filters', () => {
    it('should filter running sessions', async () => {
      const runningSession = createMockSession({ id: 'running', status: 'running' })
      const stoppedSession = createMockSession({ id: 'stopped', status: 'stopped' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([
        runningSession,
        stoppedSession,
      ])

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { runningSessions, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      expect(runningSessions.value).toHaveLength(1)
      expect(runningSessions.value[0]!.id).toBe('running')
    })

    it('should filter stopped sessions', async () => {
      const runningSession = createMockSession({ id: 'running', status: 'running' })
      const stoppedSession = createMockSession({ id: 'stopped', status: 'stopped' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([
        runningSession,
        stoppedSession,
      ])

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { stoppedSessions, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      expect(stoppedSessions.value).toHaveLength(1)
      expect(stoppedSessions.value[0]!.id).toBe('stopped')
    })
  })

  describe('updateSessionLocal', () => {
    it('should update session in local state without API call', async () => {
      const originalSession = createMockSession({ id: 'session-1', name: 'Original' })
      const terminalApi = await import('@/api/terminal-sessions')
      vi.mocked(terminalApi.listTerminalSessions).mockResolvedValue([originalSession])

      const { useTerminalSessions } = await import('../useTerminalSessions')
      const { sessions, updateSessionLocal, refreshSessions } = useTerminalSessions()

      await refreshSessions()

      const updatedSession = { ...originalSession, name: 'Updated' }
      updateSessionLocal(updatedSession)

      expect(sessions.value[0]!.name).toBe('Updated')
      // Should not call API
      expect(terminalApi.renameTerminalSession).not.toHaveBeenCalled()
    })
  })
})
