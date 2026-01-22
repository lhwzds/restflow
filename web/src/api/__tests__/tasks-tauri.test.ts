import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock modules
vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(),
  tauriInvoke: vi.fn(),
}))

vi.mock('../config', async () => {
  const actual = await vi.importActual('../config')
  return {
    ...actual,
    isTauri: vi.fn(),
    tauriInvoke: vi.fn(),
    apiClient: {
      get: vi.fn(),
      post: vi.fn(),
    },
  }
})

describe('Tasks API - Tauri Mode', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('testNodeExecution', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { testNodeExecution } = await import('../tasks')

      await expect(
        testNodeExecution({
          nodes: [],
          edges: [],
          input: {},
        }),
      ).rejects.toThrow('Test node execution is not yet supported in Tauri mode')
    })
  })
})
