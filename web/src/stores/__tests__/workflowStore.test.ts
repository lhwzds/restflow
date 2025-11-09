import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useWorkflowStore } from '../workflowStore'
import { createMockNode, createMockEdge } from '@/__tests__/helpers/testUtils'

describe('workflowStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  describe('initial state', () => {
    it('should have correct initial state', () => {
      const store = useWorkflowStore()

      expect(store.nodes).toEqual([])
      expect(store.edges).toEqual([])
      expect(store.hasUnsavedChanges).toBe(false)
      expect(store.currentWorkflowId).toBeNull()
      expect(store.currentWorkflowName).toBe('Untitled Workflow')
    })
  })

  describe('clearCanvas', () => {
    it('should clear nodes array', () => {
      const store = useWorkflowStore()
      store.nodes = [createMockNode()]

      store.clearCanvas()

      expect(store.nodes).toEqual([])
    })

    it('should clear edges array', () => {
      const store = useWorkflowStore()
      store.edges = [createMockEdge()]

      store.clearCanvas()

      expect(store.edges).toEqual([])
    })

    it('should reset hasUnsavedChanges to false', () => {
      const store = useWorkflowStore()
      store.hasUnsavedChanges = true

      store.clearCanvas()

      expect(store.hasUnsavedChanges).toBe(false)
    })

    it('should preserve currentWorkflowId', () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'
      store.nodes = [createMockNode()]

      store.clearCanvas()

      expect(store.currentWorkflowId).toBe('workflow-123')
    })

    it('should preserve currentWorkflowName', () => {
      const store = useWorkflowStore()
      store.currentWorkflowName = 'My Workflow'
      store.nodes = [createMockNode()]

      store.clearCanvas()

      expect(store.currentWorkflowName).toBe('My Workflow')
    })
  })

  describe('resetWorkflow', () => {
    it('should clear nodes and edges', () => {
      const store = useWorkflowStore()
      store.nodes = [createMockNode()]
      store.edges = [createMockEdge()]

      store.resetWorkflow()

      expect(store.nodes).toEqual([])
      expect(store.edges).toEqual([])
    })

    it('should reset hasUnsavedChanges to false', () => {
      const store = useWorkflowStore()
      store.hasUnsavedChanges = true

      store.resetWorkflow()

      expect(store.hasUnsavedChanges).toBe(false)
    })

    it('should reset currentWorkflowId to null', () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'workflow-123'

      store.resetWorkflow()

      expect(store.currentWorkflowId).toBeNull()
    })

    it('should reset currentWorkflowName to default', () => {
      const store = useWorkflowStore()
      store.currentWorkflowName = 'My Workflow'

      store.resetWorkflow()

      expect(store.currentWorkflowName).toBe('Untitled Workflow')
    })

    it('should perform full reset including metadata', () => {
      const store = useWorkflowStore()
      store.nodes = [createMockNode()]
      store.edges = [createMockEdge()]
      store.currentWorkflowId = 'workflow-123'
      store.currentWorkflowName = 'My Workflow'
      store.hasUnsavedChanges = true

      store.resetWorkflow()

      expect(store.nodes).toEqual([])
      expect(store.edges).toEqual([])
      expect(store.hasUnsavedChanges).toBe(false)
      expect(store.currentWorkflowId).toBeNull()
      expect(store.currentWorkflowName).toBe('Untitled Workflow')
    })
  })

  describe('setWorkflowMetadata', () => {
    it('should update currentWorkflowId', () => {
      const store = useWorkflowStore()

      store.setWorkflowMetadata('workflow-456', 'Test Workflow')

      expect(store.currentWorkflowId).toBe('workflow-456')
    })

    it('should update currentWorkflowName', () => {
      const store = useWorkflowStore()

      store.setWorkflowMetadata('workflow-456', 'Test Workflow')

      expect(store.currentWorkflowName).toBe('Test Workflow')
    })

    it('should handle null ID', () => {
      const store = useWorkflowStore()
      store.currentWorkflowId = 'existing-id'

      store.setWorkflowMetadata(null, 'New Workflow')

      expect(store.currentWorkflowId).toBeNull()
      expect(store.currentWorkflowName).toBe('New Workflow')
    })

    it('should handle empty name', () => {
      const store = useWorkflowStore()

      store.setWorkflowMetadata('workflow-789', '')

      expect(store.currentWorkflowId).toBe('workflow-789')
      expect(store.currentWorkflowName).toBe('')
    })
  })

  describe('node operations', () => {
    it('should add node and mark as dirty', () => {
      const store = useWorkflowStore()
      const node = createMockNode()

      store.addNode(node)

      expect(store.nodes.length).toBe(1)
      expect(store.nodes[0]?.id).toBe(node.id)
      expect(store.hasUnsavedChanges).toBe(true)
    })

    it('should remove node and mark as dirty', () => {
      const store = useWorkflowStore()
      const node = createMockNode({ id: 'node-to-remove' })
      store.nodes = [node]
      store.hasUnsavedChanges = false

      store.removeNode('node-to-remove')

      expect(store.nodes.length).toBe(0)
      expect(store.hasUnsavedChanges).toBe(true)
    })
  })

  describe('edge operations', () => {
    it('should add edge and mark as dirty', () => {
      const store = useWorkflowStore()
      const edge = createMockEdge()

      store.addEdge(edge)

      expect(store.edges.length).toBe(1)
      expect(store.edges[0]?.id).toBe(edge.id)
      expect(store.hasUnsavedChanges).toBe(true)
    })

    it('should remove edge and mark as dirty', () => {
      const store = useWorkflowStore()
      const edge = createMockEdge({ id: 'edge-to-remove' })
      store.edges = [edge]
      store.hasUnsavedChanges = false

      store.removeEdge('edge-to-remove')

      expect(store.edges.length).toBe(0)
      expect(store.hasUnsavedChanges).toBe(true)
    })
  })

  describe('loadWorkflow', () => {
    it('should load nodes and edges', () => {
      const store = useWorkflowStore()
      const nodes = [createMockNode()]
      const edges = [createMockEdge()]

      store.loadWorkflow(nodes, edges)

      expect(store.nodes).toEqual(nodes)
      expect(store.edges).toEqual(edges)
    })

    it('should mark as saved after loading', () => {
      const store = useWorkflowStore()
      store.hasUnsavedChanges = true

      store.loadWorkflow([], [])

      expect(store.hasUnsavedChanges).toBe(false)
    })
  })

  describe('dirty flag management', () => {
    it('should mark as saved', () => {
      const store = useWorkflowStore()
      store.hasUnsavedChanges = true

      store.markAsSaved()

      expect(store.hasUnsavedChanges).toBe(false)
    })

    it('should mark as dirty', () => {
      const store = useWorkflowStore()
      store.hasUnsavedChanges = false

      store.markAsDirty()

      expect(store.hasUnsavedChanges).toBe(true)
    })
  })
})
