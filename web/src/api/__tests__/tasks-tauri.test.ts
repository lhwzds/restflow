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

  describe('getTaskStatus', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { getTaskStatus } = await import('../tasks')

      await expect(getTaskStatus('task-1')).rejects.toThrow(
        'Task status is not yet supported in Tauri mode',
      )
    })
  })

  describe('listTasks', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { listTasks } = await import('../tasks')

      await expect(listTasks()).rejects.toThrow('List tasks is not yet supported in Tauri mode')
    })
  })

  describe('getExecutionStatus', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { getExecutionStatus } = await import('../tasks')

      await expect(getExecutionStatus('exec-1')).rejects.toThrow(
        'Execution status is not yet supported in Tauri mode',
      )
    })
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

  describe('executeNode', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { executeNode } = await import('../tasks')

      await expect(executeNode({ type: 'test' })).rejects.toThrow(
        'Execute node is not yet supported in Tauri mode',
      )
    })
  })
})
