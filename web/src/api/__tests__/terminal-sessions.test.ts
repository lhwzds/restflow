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

    it('should return empty array when not in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { listTerminalSessions } = await import('../terminal-sessions')
      const result = await listTerminalSessions()

      // Web mode returns mock sessions array (initially empty)
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

    it('should throw error when session not found in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getTerminalSession } = await import('../terminal-sessions')
      // Web mode throws when session not found in mock sessions
      await expect(getTerminalSession('terminal-abc123')).rejects.toThrow(
        'Terminal session not found: terminal-abc123',
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

    it('should throw error when session not found in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { renameTerminalSession } = await import('../terminal-sessions')
      // Web mode throws when session not found
      await expect(renameTerminalSession('terminal-abc123', 'New Name')).rejects.toThrow(
        'Terminal session not found: terminal-abc123',
      )
    })
  })

  describe('updateTerminalSession', () => {
    it('should update session with working_directory and startup_command', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      const updatedSession = {
        ...mockSession,
        working_directory: '~/projects',
        startup_command: 'npm run dev',
      }
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(updatedSession)

      const { updateTerminalSession } = await import('../terminal-sessions')
      const result = await updateTerminalSession('terminal-abc123', {
        working_directory: '~/projects',
        startup_command: 'npm run dev',
      })

      expect(tauriInvoke).toHaveBeenCalledWith('update_terminal_session', {
        id: 'terminal-abc123',
        name: undefined,
        workingDirectory: '~/projects',
        startupCommand: 'npm run dev',
      })
      expect(result).toEqual(updatedSession)
    })

    it('should update session with only name', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      const updatedSession = { ...mockSession, name: 'Dev Server' }
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(updatedSession)

      const { updateTerminalSession } = await import('../terminal-sessions')
      const result = await updateTerminalSession('terminal-abc123', { name: 'Dev Server' })

      expect(tauriInvoke).toHaveBeenCalledWith('update_terminal_session', {
        id: 'terminal-abc123',
        name: 'Dev Server',
        workingDirectory: undefined,
        startupCommand: undefined,
      })
      expect(result).toEqual(updatedSession)
    })

    it('should throw error when session not found in web mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { updateTerminalSession } = await import('../terminal-sessions')
      // Web mode throws when session not found
      await expect(
        updateTerminalSession('terminal-abc123', { working_directory: '~/projects' }),
      ).rejects.toThrow('Terminal session not found: terminal-abc123')
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

    it('should silently succeed in web mode (no-op if session not found)', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { deleteTerminalSession } = await import('../terminal-sessions')
      // Web mode silently succeeds (removes from mock sessions if exists)
      await expect(deleteTerminalSession('terminal-abc123')).resolves.toBeUndefined()
    })
  })
})
