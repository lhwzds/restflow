import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// Mock modules before imports
vi.mock('../config', () => ({
  isTauri: vi.fn(),
  tauriInvoke: vi.fn(),
}))

// Mock session data
const mockSession: TerminalSession = {
  id: 'terminal-abc123',
  name: 'Terminal 1',
  status: 'running',
  created_at: 1000,
  history: null,
  stopped_at: null,
  working_directory: null,
  startup_command: null,
}

describe('Terminal Sessions API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('listTerminalSessions', () => {
    it('should return sessions from tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue([mockSession])

      const { listTerminalSessions } = await import('../terminal-sessions')
      const result = await listTerminalSessions()

      expect(tauriInvoke).toHaveBeenCalledWith('list_terminal_sessions')
      expect(result).toEqual([mockSession])
    })

    it('should return empty array when not in Tauri mode (web mode)', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { listTerminalSessions } = await import('../terminal-sessions')
      const result = await listTerminalSessions()

      // Web mode returns mock sessions (initially empty)
      expect(result).toEqual([])
    })
  })

  describe('getTerminalSession', () => {
    it('should return session from tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockSession)

      const { getTerminalSession } = await import('../terminal-sessions')
      const result = await getTerminalSession('terminal-abc123')

      expect(tauriInvoke).toHaveBeenCalledWith('get_terminal_session', { id: 'terminal-abc123' })
      expect(result).toEqual(mockSession)
    })

    it('should throw error for non-existent session in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getTerminalSession } = await import('../terminal-sessions')
      await expect(getTerminalSession('non-existent-id')).rejects.toThrow(
        'Terminal session not found: non-existent-id',
      )
    })
  })

  describe('createTerminalSession', () => {
    it('should return new session from tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockSession)

      const { createTerminalSession } = await import('../terminal-sessions')
      const result = await createTerminalSession()

      expect(tauriInvoke).toHaveBeenCalledWith('create_terminal_session')
      expect(result).toEqual(mockSession)
    })

    it('should create mock session in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { createTerminalSession } = await import('../terminal-sessions')
      const result = await createTerminalSession()

      // Web mode creates a mock session
      expect(result).toHaveProperty('id')
      expect(result).toHaveProperty('name')
      expect(result.status).toBe('running')
    })
  })

  describe('renameTerminalSession', () => {
    it('should return updated session from tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      const updatedSession = { ...mockSession, name: 'New Name' }
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(updatedSession)

      const { renameTerminalSession } = await import('../terminal-sessions')
      const result = await renameTerminalSession('terminal-abc123', 'New Name')

      expect(tauriInvoke).toHaveBeenCalledWith('rename_terminal_session', {
        id: 'terminal-abc123',
        name: 'New Name',
      })
      expect(result).toEqual(updatedSession)
    })

    it('should throw error for non-existent session in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { renameTerminalSession } = await import('../terminal-sessions')
      await expect(renameTerminalSession('non-existent-id', 'New Name')).rejects.toThrow(
        'Terminal session not found: non-existent-id',
      )
    })
  })

  describe('updateTerminalSession', () => {
    it('should return updated session from tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      const updatedSession = { ...mockSession, working_directory: '/home/user' }
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(updatedSession)

      const { updateTerminalSession } = await import('../terminal-sessions')
      const result = await updateTerminalSession('terminal-abc123', {
        working_directory: '/home/user',
      })

      expect(tauriInvoke).toHaveBeenCalledWith('update_terminal_session', {
        id: 'terminal-abc123',
        name: undefined,
        workingDirectory: '/home/user',
        startupCommand: undefined,
      })
      expect(result).toEqual(updatedSession)
    })

    it('should throw error for non-existent session in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { updateTerminalSession } = await import('../terminal-sessions')
      await expect(
        updateTerminalSession('non-existent-id', { name: 'New Name' }),
      ).rejects.toThrow('Terminal session not found: non-existent-id')
    })
  })

  describe('deleteTerminalSession', () => {
    it('should call tauriInvoke in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(undefined)

      const { deleteTerminalSession } = await import('../terminal-sessions')
      await deleteTerminalSession('terminal-abc123')

      expect(tauriInvoke).toHaveBeenCalledWith('delete_terminal_session', { id: 'terminal-abc123' })
    })

    it('should not throw for non-existent session in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { deleteTerminalSession } = await import('../terminal-sessions')
      // Should not throw - silently ignores non-existent sessions
      await expect(deleteTerminalSession('non-existent-id')).resolves.toBeUndefined()
    })
  })

  describe('Web mode mock session lifecycle', () => {
    it('should create, list, rename, and delete mock sessions', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const {
        createTerminalSession,
        listTerminalSessions,
        renameTerminalSession,
        deleteTerminalSession,
      } = await import('../terminal-sessions')

      // Create a session
      const session = await createTerminalSession()
      expect(session.status).toBe('running')

      // List should include the new session
      let sessions = await listTerminalSessions()
      expect(sessions.length).toBeGreaterThanOrEqual(1)

      // Rename the session
      const renamed = await renameTerminalSession(session.id, 'Renamed Terminal')
      expect(renamed.name).toBe('Renamed Terminal')

      // Delete the session
      await deleteTerminalSession(session.id)
    })
  })
})
