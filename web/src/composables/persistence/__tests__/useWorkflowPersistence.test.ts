import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useWorkflowStore } from '@/stores/workflowStore'
import { useWorkflowPersistence } from '../useWorkflowPersistence'
import { createMockNode, createMockEdge, createMockWorkflow } from '@/__tests__/helpers/testUtils'
import * as workflowsApi from '@/api/workflows'
import { ElMessage } from 'element-plus'

// Mock the API module
vi.mock('@/api/workflows', () => ({
  updateWorkflow: vi.fn(),
  getWorkflow: vi.fn(),
  createWorkflow: vi.fn(),
}))

// Note: ElMessage is already mocked globally in tests/setup.ts

// Mock the workflow converter
vi.mock('@/composables/workflow/useWorkflowConverter', () => ({
  useWorkflowConverter: () => ({
    convertToBackendFormat: vi.fn((nodes, edges, meta) => ({
      id: meta.id,
      name: meta.name,
      nodes,
      edges,
    })),
    convertToVueFlowFormat: vi.fn((workflow) => ({
      nodes: workflow.nodes || [],
      edges: workflow.edges || [],
    })),
  }),
}))

describe('useWorkflowPersistence', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  describe('saveWorkflow', () => {
    it('should throw error if currentWorkflowId is null', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = null
      store.currentWorkflowName = 'Test Workflow'

      const { saveWorkflow } = useWorkflowPersistence()
      const result = await saveWorkflow([], [], { showMessage: false })

      expect(result.success).toBe(false)
      expect(result.error).toContain('Cannot save workflow without ID')
    })

    it('should call updateWorkflow API when saving', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'

      const mockUpdateWorkflow = vi.mocked(workflowsApi.updateWorkflow)
      mockUpdateWorkflow.mockResolvedValue(undefined)

      const { saveWorkflow } = useWorkflowPersistence()
      const nodes = [createMockNode()]
      const edges = [createMockEdge()]

      const result = await saveWorkflow(nodes, edges, { showMessage: false })

      expect(mockUpdateWorkflow).toHaveBeenCalledWith(
        'workflow-123',
        expect.objectContaining({
          id: 'workflow-123',
          name: 'My Workflow',
          nodes: expect.arrayContaining([
            expect.objectContaining({
              id: 'node-1',
              node_type: 'Agent',
            }),
          ]),
          edges: expect.arrayContaining([
            expect.objectContaining({
              from: 'node-1',
              to: 'node-2',
            }),
          ]),
        }),
      )
      expect(result.success).toBe(true)
    })

    it('should update store metadata after successful save', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'

      const mockUpdateWorkflow = vi.mocked(workflowsApi.updateWorkflow)
      mockUpdateWorkflow.mockResolvedValue(undefined)

      const { saveWorkflow } = useWorkflowPersistence()
      await saveWorkflow([], [], { showMessage: false })

      expect(store.currentWorkflowId).toBe('workflow-123')
      expect(store.currentWorkflowName).toBe('My Workflow')
    })

    it('should show success message when showMessage is true', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'

      const mockUpdateWorkflow = vi.mocked(workflowsApi.updateWorkflow)
      mockUpdateWorkflow.mockResolvedValue(undefined)

      const { saveWorkflow } = useWorkflowPersistence()
      await saveWorkflow([], [], { showMessage: true })

      expect(ElMessage.success).toHaveBeenCalled()
    })

    it('should not show success message when showMessage is false', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'

      const mockUpdateWorkflow = vi.mocked(workflowsApi.updateWorkflow)
      mockUpdateWorkflow.mockResolvedValue(undefined)

      const { saveWorkflow } = useWorkflowPersistence()
      await saveWorkflow([], [], { showMessage: false })

      expect(ElMessage.success).not.toHaveBeenCalled()
    })

    it('should handle API errors gracefully', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'

      const mockUpdateWorkflow = vi.mocked(workflowsApi.updateWorkflow)
      mockUpdateWorkflow.mockRejectedValue(new Error('API Error'))

      const { saveWorkflow } = useWorkflowPersistence()
      const result = await saveWorkflow([], [], { showMessage: false })

      expect(result.success).toBe(false)
      expect(result.error).toBeDefined()
    })

    it('should validate workflow name is not empty', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = ''

      const { saveWorkflow } = useWorkflowPersistence()
      const result = await saveWorkflow([], [], { showMessage: false })

      expect(result.success).toBe(false)
      expect(result.error).toContain('Name is required')
    })

    it('should validate workflow name is not whitespace only', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = '   '

      const { saveWorkflow } = useWorkflowPersistence()
      const result = await saveWorkflow([], [], { showMessage: false })

      expect(result.success).toBe(false)
      expect(result.error).toContain('Name is required')
    })
  })

  describe('loadWorkflow', () => {
    it('should fetch workflow by ID', async () => {
      const mockGetWorkflow = vi.mocked(workflowsApi.getWorkflow)
      const mockWorkflow = createMockWorkflow({
        id: 'workflow-123',
        name: 'Test Workflow',
      })
      mockGetWorkflow.mockResolvedValue(mockWorkflow)

      const { loadWorkflow } = useWorkflowPersistence()
      const result = await loadWorkflow('workflow-123')

      expect(mockGetWorkflow).toHaveBeenCalledWith('workflow-123')
      expect(result.success).toBe(true)
    })

    it('should update store with loaded workflow', async () => {
      const store = useWorkflowStore()
      const mockGetWorkflow = vi.mocked(workflowsApi.getWorkflow)
      const mockWorkflow = createMockWorkflow({
        id: 'workflow-123',
        name: 'Test Workflow',
        nodes: [],
        edges: [],
      })
      mockGetWorkflow.mockResolvedValue(mockWorkflow)

      const { loadWorkflow } = useWorkflowPersistence()
      await loadWorkflow('workflow-123')

      expect(store.currentWorkflowId).toBe('workflow-123')
      expect(store.currentWorkflowName).toBe('Test Workflow')
    })

    it('should handle non-existent workflow gracefully', async () => {
      const mockGetWorkflow = vi.mocked(workflowsApi.getWorkflow)
      mockGetWorkflow.mockRejectedValue(new Error('Workflow not found'))

      const { loadWorkflow } = useWorkflowPersistence()
      const result = await loadWorkflow('non-existent-id')

      expect(result.success).toBe(false)
      expect(result.error).toBeDefined()
    })
  })

  describe('createNewWorkflow', () => {
    it('should reset workflow store', () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'Old Workflow'
      store.nodes = [createMockNode()]
      store.edges = [createMockEdge()]

      const { createNewWorkflow } = useWorkflowPersistence()
      createNewWorkflow()

      expect(store.currentWorkflowId).toBeNull()
      expect(store.currentWorkflowName).toBe('Untitled Workflow')
      expect(store.nodes).toEqual([])
      expect(store.edges).toEqual([])
    })
  })

  describe('saveAsNew', () => {
    it('should create new workflow with current nodes/edges', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'original-workflow-123'
      store.currentWorkflowName = 'Original Workflow'
      store.nodes = [createMockNode()]
      store.edges = [createMockEdge()]

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-456' })

      const { saveAsNew } = useWorkflowPersistence()
      const result = await saveAsNew('Duplicated Workflow')

      expect(mockCreateWorkflow).toHaveBeenCalledWith(
        expect.objectContaining({
          name: 'Duplicated Workflow',
          id: expect.stringMatching(/^workflow-\d+-[a-z0-9]+$/),
          nodes: expect.any(Array),
          edges: expect.any(Array),
        }),
      )
      // Verify it was called with the workflow object
      expect(mockCreateWorkflow).toHaveBeenCalledTimes(1)
      expect(result.success).toBe(true)
      expect(result.id).toBe('new-workflow-456')
    })

    it('should generate unique workflow ID', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-789' })

      const { saveAsNew } = useWorkflowPersistence()
      await saveAsNew('New Workflow')

      expect(mockCreateWorkflow).toHaveBeenCalledWith(
        expect.objectContaining({
          id: expect.stringMatching(/^workflow-\d+-[a-z0-9]+$/),
          name: 'New Workflow',
          nodes: expect.any(Array),
          edges: expect.any(Array),
        }),
      )
    })

    it('should update store with new workflow metadata', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'old-workflow-123'
      store.currentWorkflowName = 'Old Workflow'

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-456' })

      const { saveAsNew } = useWorkflowPersistence()
      await saveAsNew('New Workflow Name')

      expect(store.currentWorkflowId).toBe('new-workflow-456')
      expect(store.currentWorkflowName).toBe('New Workflow Name')
    })

    it('should show success message', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-456' })

      const { saveAsNew } = useWorkflowPersistence()
      await saveAsNew('Saved Workflow')

      expect(ElMessage.success).toHaveBeenCalled()
    })

    it('should validate workflow name is not empty', async () => {
      const { saveAsNew } = useWorkflowPersistence()
      const result = await saveAsNew('')

      expect(result.success).toBe(false)
      expect(result.error).toBe('Name is required')
      expect(vi.mocked(ElMessage.error)).toHaveBeenCalled()
    })

    it('should validate workflow name is not whitespace', async () => {
      const { saveAsNew } = useWorkflowPersistence()
      const result = await saveAsNew('   ')

      expect(result.success).toBe(false)
      expect(result.error).toBe('Name is required')
      expect(vi.mocked(ElMessage.error)).toHaveBeenCalled()
    })

    it('should handle API errors gracefully', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'Old Workflow'

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockRejectedValue(new Error('API Error'))

      const { saveAsNew } = useWorkflowPersistence()
      const result = await saveAsNew('Failed Workflow')

      expect(result.success).toBe(false)
      expect(result.error).toBeDefined()
      expect(vi.mocked(ElMessage.error)).toHaveBeenCalled()
    })

    it('should not modify store metadata if save fails', async () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'original-workflow-123'
      store.currentWorkflowName = 'Original Workflow'

      const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
      mockCreateWorkflow.mockRejectedValue(new Error('Network error'))

      const { saveAsNew } = useWorkflowPersistence()
      await saveAsNew('Failed Workflow')

      // Store metadata should remain unchanged
      expect(store.currentWorkflowId).toBe('original-workflow-123')
      expect(store.currentWorkflowName).toBe('Original Workflow')
    })
  })
})
