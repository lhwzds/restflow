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
  updated_at: 2000,
  history: null,
  stopped_at: null,
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

      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

      const { listTerminalSessions } = await import('../terminal-sessions')
      const result = await listTerminalSessions()

      expect(result).toEqual([])
      expect(consoleSpy).toHaveBeenCalledWith('Terminal sessions are only available in Tauri mode')

      consoleSpy.mockRestore()
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

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getTerminalSession } = await import('../terminal-sessions')
      await expect(getTerminalSession('terminal-abc123')).rejects.toThrow(
        'Terminal sessions are only available in Tauri mode',
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

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { createTerminalSession } = await import('../terminal-sessions')
      await expect(createTerminalSession()).rejects.toThrow(
        'Terminal sessions are only available in Tauri mode',
      )
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

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { renameTerminalSession } = await import('../terminal-sessions')
      await expect(renameTerminalSession('terminal-abc123', 'New Name')).rejects.toThrow(
        'Terminal sessions are only available in Tauri mode',
      )
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

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { deleteTerminalSession } = await import('../terminal-sessions')
      await expect(deleteTerminalSession('terminal-abc123')).rejects.toThrow(
        'Terminal sessions are only available in Tauri mode',
      )
    })
  })
})
