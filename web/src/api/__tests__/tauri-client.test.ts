import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { isTauri, tauriInvoke } from '../tauri-client'

// Declare global for Node.js environment in tests
declare const global: typeof globalThis

// Mock @tauri-apps/api/core
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

describe('tauri-client', () => {
  describe('isTauri', () => {
    const originalWindow = global.window

    beforeEach(() => {
      // Reset window to a clean state
      global.window = {} as Window & typeof globalThis
    })

    afterEach(() => {
      global.window = originalWindow
    })

    it('should return false when __TAURI_INTERNALS__ is not present', () => {
      expect(isTauri()).toBe(false)
    })

    it('should return false when window is undefined', () => {
      const savedWindow = global.window
      // @ts-expect-error - testing undefined window
      global.window = undefined
      expect(isTauri()).toBe(false)
      global.window = savedWindow
    })

    it('should return true when __TAURI_INTERNALS__ is present', () => {
      ;(global.window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
      expect(isTauri()).toBe(true)
    })
  })

  describe('tauriInvoke', () => {
    beforeEach(() => {
      vi.clearAllMocks()
    })

    it('should call invoke with command and args', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue({ data: 'test' })

      const result = await tauriInvoke('test_command', { arg1: 'value1' })

      expect(mockInvoke).toHaveBeenCalledWith('test_command', { arg1: 'value1' })
      expect(result).toEqual({ data: 'test' })
    })

    it('should call invoke without args when not provided', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockResolvedValue('success')

      const result = await tauriInvoke('simple_command')

      expect(mockInvoke).toHaveBeenCalledWith('simple_command', undefined)
      expect(result).toBe('success')
    })

    it('should convert string errors to Error objects', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      mockInvoke.mockRejectedValue('Something went wrong')

      await expect(tauriInvoke('failing_command')).rejects.toThrow('Something went wrong')
    })

    it('should re-throw Error objects as-is', async () => {
      const { invoke } = await import('@tauri-apps/api/core')
      const mockInvoke = vi.mocked(invoke)
      const originalError = new Error('Original error')
      mockInvoke.mockRejectedValue(originalError)

      await expect(tauriInvoke('failing_command')).rejects.toThrow(originalError)
    })
  })
})
