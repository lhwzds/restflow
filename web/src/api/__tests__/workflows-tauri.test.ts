import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionHistoryPage } from '@/types/generated/ExecutionHistoryPage'

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
      put: vi.fn(),
      delete: vi.fn(),
    },
  }
})

describe('Workflows API - Tauri Mode', () => {
  const mockWorkflow: Workflow = {
    id: 'wf-1',
    name: 'Test Workflow',
    nodes: [
      {
        id: 'node1',
        node_type: 'Agent',
        config: { model: 'gpt-4', prompt: 'test' },
        position: null,
      },
    ],
    edges: [],
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('listWorkflows', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue([mockWorkflow])

      const { listWorkflows } = await import('../workflows')
      const result = await listWorkflows()

      expect(tauriInvoke).toHaveBeenCalledWith('list_workflows')
      expect(result).toEqual([mockWorkflow])
    })
  })

  describe('getWorkflow', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockWorkflow)

      const { getWorkflow } = await import('../workflows')
      const result = await getWorkflow('wf-1')

      expect(tauriInvoke).toHaveBeenCalledWith('get_workflow', { id: 'wf-1' })
      expect(result).toEqual(mockWorkflow)
    })
  })

  describe('createWorkflow', () => {
    it('should use Tauri invoke and return id', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockWorkflow)

      const { createWorkflow } = await import('../workflows')
      const result = await createWorkflow(mockWorkflow)

      expect(tauriInvoke).toHaveBeenCalledWith('create_workflow', { workflow: mockWorkflow })
      expect(result).toEqual({ id: 'wf-1' })
    })
  })

  describe('updateWorkflow', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(mockWorkflow)

      const { updateWorkflow } = await import('../workflows')
      await updateWorkflow('wf-1', mockWorkflow)

      expect(tauriInvoke).toHaveBeenCalledWith('update_workflow', {
        id: 'wf-1',
        workflow: mockWorkflow,
      })
    })
  })

  describe('deleteWorkflow', () => {
    it('should use Tauri invoke when in Tauri mode', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue(undefined)

      const { deleteWorkflow } = await import('../workflows')
      await deleteWorkflow('wf-1')

      expect(tauriInvoke).toHaveBeenCalledWith('delete_workflow', { id: 'wf-1' })
    })
  })

  describe('executeInline', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { executeInline } = await import('../workflows')

      await expect(executeInline(mockWorkflow)).rejects.toThrow(
        'executeInline is not supported in Tauri mode',
      )
    })
  })

  describe('submitWorkflow', () => {
    it('should use Tauri invoke and return execution_id', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue('exec-123')

      const { submitWorkflow } = await import('../workflows')
      const result = await submitWorkflow('wf-1', { key: 'value' })

      expect(tauriInvoke).toHaveBeenCalledWith('execute_workflow', {
        id: 'wf-1',
        input: { key: 'value' },
      })
      expect(result).toEqual({ execution_id: 'exec-123' })
    })

    it('should pass null when no initial variables provided', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)
      vi.mocked(tauriInvoke).mockResolvedValue('exec-123')

      const { submitWorkflow } = await import('../workflows')
      await submitWorkflow('wf-1')

      expect(tauriInvoke).toHaveBeenCalledWith('execute_workflow', {
        id: 'wf-1',
        input: null,
      })
    })
  })

  describe('listWorkflowExecutions', () => {
    it('should use Tauri invoke with pagination params', async () => {
      const { isTauri, tauriInvoke } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const mockPage: ExecutionHistoryPage = {
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
        total_pages: 0,
      }
      vi.mocked(tauriInvoke).mockResolvedValue(mockPage)

      const { listWorkflowExecutions } = await import('../workflows')
      const result = await listWorkflowExecutions('wf-1', 2, 50)

      expect(tauriInvoke).toHaveBeenCalledWith('get_workflow_executions', {
        workflow_id: 'wf-1',
        page: 2,
        page_size: 50,
      })
      expect(result).toEqual(mockPage)
    })
  })
})
