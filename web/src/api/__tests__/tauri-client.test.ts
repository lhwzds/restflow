import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { isTauri, invokeCommand, tauriInvoke } from '../tauri-client'
import { commands } from '../bindings'

// Declare global for Node.js environment in tests
declare const global: typeof globalThis

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('../bindings', () => ({
  commands: {
    listAgents: vi.fn(),
    getHeartbeatEventName: vi.fn(),
  },
}))

describe('tauri-client', () => {
  describe('isTauri', () => {
    const originalWindow = global.window

    beforeEach(() => {
      global.window = {} as Window & typeof globalThis
    })

    afterEach(() => {
      global.window = originalWindow
    })

    it('returns false when __TAURI_INTERNALS__ is not present', () => {
      expect(isTauri()).toBe(false)
    })

    it('returns false when window is undefined', () => {
      const savedWindow = global.window
      // @ts-expect-error - testing undefined window
      global.window = undefined
      expect(isTauri()).toBe(false)
      global.window = savedWindow
    })

    it('returns true when __TAURI_INTERNALS__ is present', () => {
      ;(global.window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
      expect(isTauri()).toBe(true)
    })
  })

  describe('invokeCommand', () => {
    beforeEach(() => {
      vi.clearAllMocks()
    })

    it('unwraps Specta result envelope when status is ok', async () => {
      vi.mocked(commands.listAgents).mockResolvedValue({
        status: 'ok',
        data: [{ id: 'agent-1', name: 'Agent 1' }] as any,
      })

      const result = await invokeCommand('listAgents')

      expect(commands.listAgents).toHaveBeenCalledTimes(1)
      expect(result).toEqual([{ id: 'agent-1', name: 'Agent 1' }])
    })

    it('throws normalized error when Specta status is error', async () => {
      vi.mocked(commands.listAgents).mockResolvedValue({
        status: 'error',
        error: 'permission denied',
      })

      await expect(invokeCommand('listAgents')).rejects.toThrow('permission denied')
    })

    it('returns raw values for commands without Specta envelope', async () => {
      vi.mocked(commands.getHeartbeatEventName).mockResolvedValue('background-agent:heartbeat')

      const result = await invokeCommand('getHeartbeatEventName')

      expect(result).toBe('background-agent:heartbeat')
    })
  })

  describe('tauriInvoke', () => {
    beforeEach(() => {
      vi.clearAllMocks()
    })

    it('calls invoke with command and args', async () => {
      vi.mocked(invoke).mockResolvedValue({ data: 'test' })

      const result = await tauriInvoke('test_command', { arg1: 'value1' })

      expect(invoke).toHaveBeenCalledWith('test_command', { arg1: 'value1' })
      expect(result).toEqual({ data: 'test' })
    })

    it('normalizes string errors', async () => {
      vi.mocked(invoke).mockRejectedValue('Something went wrong')

      await expect(tauriInvoke('failing_command')).rejects.toThrow('Something went wrong')
    })
  })
})
