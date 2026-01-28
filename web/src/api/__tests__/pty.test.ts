import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock modules before imports
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(),
}))

describe('PTY API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('spawnPty', () => {
    it('should call invoke with correct parameters in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { spawnPty } = await import('../pty')
      await spawnPty('session-1', 80, 24)

      expect(invoke).toHaveBeenCalledWith('spawn_pty', {
        sessionId: 'session-1',
        cols: 80,
        rows: 24,
      })
    })

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { spawnPty } = await import('../pty')
      await expect(spawnPty('session-1', 80, 24)).rejects.toThrow(
        'PTY is only available in Tauri desktop app',
      )
    })
  })

  describe('writePty', () => {
    it('should call invoke with correct parameters in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { writePty } = await import('../pty')
      await writePty('session-1', 'ls -la\n')

      expect(invoke).toHaveBeenCalledWith('write_pty', {
        sessionId: 'session-1',
        data: 'ls -la\n',
      })
    })

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { writePty } = await import('../pty')
      await expect(writePty('session-1', 'test')).rejects.toThrow(
        'PTY is only available in Tauri desktop app',
      )
    })
  })

  describe('resizePty', () => {
    it('should call invoke with correct parameters in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { resizePty } = await import('../pty')
      await resizePty('session-1', 120, 40)

      expect(invoke).toHaveBeenCalledWith('resize_pty', {
        sessionId: 'session-1',
        cols: 120,
        rows: 40,
      })
    })

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { resizePty } = await import('../pty')
      await expect(resizePty('session-1', 120, 40)).rejects.toThrow(
        'PTY is only available in Tauri desktop app',
      )
    })
  })

  describe('closePty', () => {
    it('should call invoke with correct parameters in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { closePty } = await import('../pty')
      await closePty('session-1')

      expect(invoke).toHaveBeenCalledWith('close_pty', { sessionId: 'session-1' })
    })

    it('should not throw in web mode (updates mock session)', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { closePty } = await import('../pty')
      // Web mode: silently updates mock session status, doesn't throw
      await expect(closePty('session-1')).resolves.toBeUndefined()
    })
  })

  describe('getPtyStatus', () => {
    it('should return status from invoke in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(true)

      const { getPtyStatus } = await import('../pty')
      const result = await getPtyStatus('session-1')

      expect(invoke).toHaveBeenCalledWith('get_pty_status', { sessionId: 'session-1' })
      expect(result).toBe(true)
    })

    it('should return false when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getPtyStatus } = await import('../pty')
      const result = await getPtyStatus('session-1')

      expect(result).toBe(false)
    })
  })

  describe('getPtyHistory', () => {
    it('should return history from invoke in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue('$ ls\nfile1 file2\n$')

      const { getPtyHistory } = await import('../pty')
      const result = await getPtyHistory('session-1')

      expect(invoke).toHaveBeenCalledWith('get_pty_history', { sessionId: 'session-1' })
      expect(result).toBe('$ ls\nfile1 file2\n$')
    })

    it('should return empty string when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getPtyHistory } = await import('../pty')
      const result = await getPtyHistory('session-1')

      expect(result).toBe('')
    })
  })

  describe('saveTerminalHistory', () => {
    it('should call invoke in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { saveTerminalHistory } = await import('../pty')
      await saveTerminalHistory('session-1')

      expect(invoke).toHaveBeenCalledWith('save_terminal_history', { sessionId: 'session-1' })
    })

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { saveTerminalHistory } = await import('../pty')
      await expect(saveTerminalHistory('session-1')).rejects.toThrow(
        'PTY is only available in Tauri desktop app',
      )
    })
  })

  describe('saveAllTerminalHistory', () => {
    it('should call invoke in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(undefined)

      const { saveAllTerminalHistory } = await import('../pty')
      await saveAllTerminalHistory()

      expect(invoke).toHaveBeenCalledWith('save_all_terminal_history')
    })

    it('should throw error when not in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { saveAllTerminalHistory } = await import('../pty')
      await expect(saveAllTerminalHistory()).rejects.toThrow(
        'PTY is only available in Tauri desktop app',
      )
    })
  })

  describe('restartTerminal', () => {
    it('should return updated session from invoke in Tauri mode', async () => {
      const { isTauri } = await import('../tauri-client')
      const { invoke } = await import('@tauri-apps/api/core')
      const mockSession = {
        id: 'session-1',
        name: 'Terminal 1',
        status: 'running' as const,
        created_at: 1000,
        updated_at: 2000,
        history: null,
        stopped_at: null,
      }
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(invoke).mockResolvedValue(mockSession)

      const { restartTerminal } = await import('../pty')
      const result = await restartTerminal('session-1')

      expect(invoke).toHaveBeenCalledWith('restart_terminal', { sessionId: 'session-1' })
      expect(result).toEqual(mockSession)
    })

    it('should throw error for non-existent session in web mode', async () => {
      const { isTauri } = await import('../tauri-client')
      vi.mocked(isTauri).mockReturnValue(false)

      const { restartTerminal } = await import('../pty')
      // Web mode: throws if session not found in mock sessions
      await expect(restartTerminal('non-existent-session')).rejects.toThrow(
        'Terminal session not found: non-existent-session',
      )
    })
  })

  describe('onPtyOutput', () => {
    it('should register listener and filter by session id', async () => {
      const { listen } = await import('@tauri-apps/api/event')
      const mockUnlisten = vi.fn()
      vi.mocked(listen).mockResolvedValue(mockUnlisten)

      const { onPtyOutput } = await import('../pty')
      const callback = vi.fn()
      const unlisten = await onPtyOutput('session-1', callback)

      expect(listen).toHaveBeenCalledWith('pty_output', expect.any(Function))
      expect(unlisten).toBe(mockUnlisten)

      // Simulate event
      const eventHandler = vi.mocked(listen).mock.calls[0]![1]
      eventHandler({ payload: { session_id: 'session-1', data: 'output' } } as never)
      expect(callback).toHaveBeenCalledWith('output')

      // Should not call callback for different session
      eventHandler({ payload: { session_id: 'session-2', data: 'other' } } as never)
      expect(callback).toHaveBeenCalledTimes(1)
    })
  })

  describe('onPtyClosed', () => {
    it('should register listener and filter by session id', async () => {
      const { listen } = await import('@tauri-apps/api/event')
      const mockUnlisten = vi.fn()
      vi.mocked(listen).mockResolvedValue(mockUnlisten)

      const { onPtyClosed } = await import('../pty')
      const callback = vi.fn()
      const unlisten = await onPtyClosed('session-1', callback)

      expect(listen).toHaveBeenCalledWith('pty_closed', expect.any(Function))
      expect(unlisten).toBe(mockUnlisten)

      // Simulate event
      const eventHandler = vi.mocked(listen).mock.calls[0]![1]
      eventHandler({ payload: { session_id: 'session-1', data: '' } } as never)
      expect(callback).toHaveBeenCalled()

      // Should not call callback for different session
      eventHandler({ payload: { session_id: 'session-2', data: '' } } as never)
      expect(callback).toHaveBeenCalledTimes(1)
    })
  })
})
