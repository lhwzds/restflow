import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref, nextTick } from 'vue'
import { setActivePinia, createPinia } from 'pinia'
import { useWorkflowStore } from '@/stores/workflowStore'
import { useExecutionStore } from '@/stores/executionStore'
import { useAvailableVariables } from '../useAvailableVariables'
import type { Node, Edge } from '@vue-flow/core'

// Mock getNodeOutputSchema
vi.mock('@/utils/schemaGenerator', () => ({
  getNodeOutputSchema: vi.fn((nodeType: string) => {
    const schemas: Record<string, any[]> = {
      Agent: [{ name: 'response', type: 'string', path: 'data.response' }],
      ManualTrigger: [{ name: 'payload', type: 'object', path: 'data.payload' }],
      HttpRequest: [
        { name: 'status', type: 'number', path: 'data.status' },
        { name: 'body', type: 'object', path: 'data.body' },
      ],
    }
    return schemas[nodeType] || []
  }),
}))

describe('useAvailableVariables', () => {
  let workflowStore: ReturnType<typeof useWorkflowStore>
  let executionStore: ReturnType<typeof useExecutionStore>

  const createMockNode = (id: string, type: string, data: any = {}): Node => ({
    id,
    type,
    position: { x: 0, y: 0 },
    data,
    events: {},
  })

  const createMockEdge = (source: string, target: string): Edge => ({
    id: `e${source}-${target}`,
    source,
    target,
    events: {},
  })

  beforeEach(() => {
    setActivePinia(createPinia())
    workflowStore = useWorkflowStore()
    executionStore = useExecutionStore()
    vi.clearAllMocks()
  })

  describe('initialization', () => {
    it('should initialize with empty arrays', () => {
      const currentNodeId = ref<string | null>(null)
      const { availableVariables } = useAvailableVariables(currentNodeId)

      expect(availableVariables.value.trigger).toEqual([])
      expect(availableVariables.value.nodes).toEqual([])
      expect(availableVariables.value.vars).toEqual([])
      expect(availableVariables.value.config).toEqual([])
    })
  })

  describe('when no execution results', () => {
    it('should use schema-based fields for upstream nodes', () => {
      // Setup workflow: Trigger → Agent → CurrentNode
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agentNode = createMockNode('agent-1', 'Agent', { model: 'gpt-4' })
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agentNode, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'current-1'),
      ]

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Should have 2 upstream nodes (trigger and agent)
      expect(availableVariables.value.nodes).toHaveLength(2)

      // Trigger should not be in trigger array (now unified)
      expect(availableVariables.value.trigger).toEqual([])

      // Should have ManualTrigger in nodes array
      const triggerVar = availableVariables.value.nodes.find((n) => n.id === 'trigger-1')
      expect(triggerVar).toBeDefined()
      expect(triggerVar?.type).toBe('ManualTrigger')

      // Should have Agent in nodes array
      const agentVar = availableVariables.value.nodes.find((n) => n.id === 'agent-1')
      expect(agentVar).toBeDefined()
      expect(agentVar?.type).toBe('Agent')
    })

    it('should generate correct node.{id} path format', () => {
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, currentNode]
      workflowStore.edges = [createMockEdge('trigger-1', 'current-1')]

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      const triggerVar = availableVariables.value.nodes.find((n) => n.id === 'trigger-1')
      expect(triggerVar?.fields).toBeDefined()

      // All paths should start with node.trigger-1
      triggerVar?.fields.forEach((field) => {
        expect(field.path).toMatch(/^node\.trigger-1/)
      })
    })
  })

  describe('when execution results exist', () => {
    it('should use node output instead of input', () => {
      // Setup workflow
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agentNode = createMockNode('agent-1', 'Agent')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agentNode, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'current-1'),
      ]

      // Setup execution results with output (not input!)
      executionStore.nodeResults = new Map([
        [
          'trigger-1',
          {
            status: 'Completed',
            output: {
              type: 'ManualTrigger',
              data: {
                triggered_at: 1234567890,
                payload: { message: 'test trigger output' },
              },
            },
            input: null, // This should NOT be used
          } as any,
        ],
        [
          'agent-1',
          {
            status: 'Completed',
            output: {
              type: 'Agent',
              data: {
                response: 'AI response from output',
              },
            },
            input: null,
          } as any,
        ],
      ])
      executionStore.nodeResultsVersion++

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Should have nodes from execution results
      expect(availableVariables.value.nodes).toHaveLength(2)

      // Find the agent node
      const agentVar = availableVariables.value.nodes.find((n) => n.id === 'agent-1')
      expect(agentVar).toBeDefined()

      // Should have fields parsed from output
      // The structure is: output: { type, data: { response } }
      // So paths will be like: node.agent-1.type, node.agent-1.data, node.agent-1.data.response

      // Check that we have a 'data' field
      const dataField = agentVar?.fields.find((f) => f.name === 'data')
      expect(dataField).toBeDefined()

      // Check the nested response field
      const responseField = dataField?.children?.find((f) => f.name === 'response')
      expect(responseField).toBeDefined()
      expect(responseField?.value).toBe('AI response from output')
    })

    it('should not duplicate Trigger in both trigger and nodes arrays', () => {
      // Setup workflow: Trigger → Agent → CurrentNode
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agentNode = createMockNode('agent-1', 'Agent')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agentNode, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'current-1'),
      ]

      // Setup execution results
      executionStore.nodeResults = new Map([
        [
          'trigger-1',
          {
            status: 'Completed',
            output: {
              type: 'ManualTrigger',
              data: {
                triggered_at: 1234567890,
                payload: { message: 'test' },
              },
            },
          } as any,
        ],
        [
          'agent-1',
          {
            status: 'Completed',
            output: {
              type: 'Agent',
              data: { response: 'AI response' },
            },
          } as any,
        ],
      ])
      executionStore.nodeResultsVersion++

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Key assertion: trigger array should be empty (unified handling)
      expect(availableVariables.value.trigger).toEqual([])

      // Trigger should be in nodes array only once
      const triggerNodes = availableVariables.value.nodes.filter((n) => n.id === 'trigger-1')
      expect(triggerNodes).toHaveLength(1)

      // Agent should also be in nodes array
      const agentNodes = availableVariables.value.nodes.filter((n) => n.id === 'agent-1')
      expect(agentNodes).toHaveLength(1)

      // Total should be 2 (not 3 which would indicate duplication)
      expect(availableVariables.value.nodes).toHaveLength(2)
    })

    it('should generate correct path format for all nodes', () => {
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agentNode = createMockNode('agent-1', 'Agent')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agentNode, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'current-1'),
      ]

      executionStore.nodeResults = new Map([
        [
          'trigger-1',
          {
            status: 'Completed',
            output: {
              type: 'ManualTrigger',
              data: { payload: { test: 'value' } },
            },
          } as any,
        ],
        [
          'agent-1',
          {
            status: 'Completed',
            output: {
              type: 'Agent',
              data: { response: 'test' },
            },
          } as any,
        ],
      ])
      executionStore.nodeResultsVersion++

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // All nodes should have paths starting with node.{nodeId}
      availableVariables.value.nodes.forEach((node) => {
        node.fields.forEach((field) => {
          expect(field.path).toMatch(/^node\.[a-z0-9-]+/)
          expect(field.path).toContain(`node.${node.id}`)
        })
      })
    })
  })

  describe('upstream node detection', () => {
    it('should find all upstream nodes including Trigger', () => {
      // Setup: Trigger → Agent1 → Agent2 → CurrentNode
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agent1Node = createMockNode('agent-1', 'Agent')
      const agent2Node = createMockNode('agent-2', 'Agent')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agent1Node, agent2Node, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'agent-2'),
        createMockEdge('agent-2', 'current-1'),
      ]

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Should find all 3 upstream nodes
      expect(availableVariables.value.nodes).toHaveLength(3)

      const nodeIds = availableVariables.value.nodes.map((n) => n.id)
      expect(nodeIds).toContain('trigger-1')
      expect(nodeIds).toContain('agent-1')
      expect(nodeIds).toContain('agent-2')
    })

    it('should not duplicate nodes in upstream list', () => {
      // Setup a diamond pattern:
      //    Trigger
      //    /    \
      //  A1      A2
      //    \    /
      //   Current
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agent1Node = createMockNode('agent-1', 'Agent')
      const agent2Node = createMockNode('agent-2', 'Agent')
      const currentNode = createMockNode('current-1', 'Agent')

      workflowStore.nodes = [triggerNode, agent1Node, agent2Node, currentNode]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('trigger-1', 'agent-2'),
        createMockEdge('agent-1', 'current-1'),
        createMockEdge('agent-2', 'current-1'),
      ]

      const currentNodeId = ref('current-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Should have 3 unique nodes (trigger appears once despite 2 paths)
      expect(availableVariables.value.nodes).toHaveLength(3)

      const triggerNodes = availableVariables.value.nodes.filter((n) => n.id === 'trigger-1')
      expect(triggerNodes).toHaveLength(1) // Should not duplicate
    })

    it('should handle node with no upstream nodes', () => {
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      workflowStore.nodes = [triggerNode]
      workflowStore.edges = []

      const currentNodeId = ref('trigger-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Trigger has no upstream nodes
      expect(availableVariables.value.nodes).toHaveLength(0)
      expect(availableVariables.value.trigger).toEqual([])
    })
  })

  describe('reactivity', () => {
    it('should update when currentNodeId changes', async () => {
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agent1Node = createMockNode('agent-1', 'Agent')
      const agent2Node = createMockNode('agent-2', 'Agent')

      workflowStore.nodes = [triggerNode, agent1Node, agent2Node]
      workflowStore.edges = [
        createMockEdge('trigger-1', 'agent-1'),
        createMockEdge('agent-1', 'agent-2'),
      ]

      const currentNodeId = ref<string | null>('agent-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Initially viewing agent-1: should have trigger as upstream
      expect(availableVariables.value.nodes).toHaveLength(1)
      expect(availableVariables.value.nodes[0]?.id).toBe('trigger-1')

      // Change to agent-2: should have trigger and agent-1 as upstream
      currentNodeId.value = 'agent-2'
      await nextTick()

      expect(availableVariables.value.nodes).toHaveLength(2)
      const nodeIds = availableVariables.value.nodes.map((n) => n.id)
      expect(nodeIds).toContain('trigger-1')
      expect(nodeIds).toContain('agent-1')
    })

    it('should update when execution results change', async () => {
      const triggerNode = createMockNode('trigger-1', 'ManualTrigger')
      const agentNode = createMockNode('agent-1', 'Agent')

      workflowStore.nodes = [triggerNode, agentNode]
      workflowStore.edges = [createMockEdge('trigger-1', 'agent-1')]

      const currentNodeId = ref('agent-1')
      const { availableVariables } = useAvailableVariables(currentNodeId)

      // Initially no execution results (should use schema)
      expect(availableVariables.value.nodes).toHaveLength(1)
      const initialFields = availableVariables.value.nodes[0]?.fields || []
      expect(initialFields.some((f) => f.value === undefined)).toBe(true) // Schema fields

      // Add execution results
      executionStore.nodeResults = new Map([
        [
          'trigger-1',
          {
            status: 'Completed',
            output: {
              type: 'ManualTrigger',
              data: { payload: { realValue: 'from execution' } },
            },
          } as any,
        ],
      ])
      executionStore.nodeResultsVersion++

      await nextTick()

      // Should now have actual values from execution
      const updatedFields = availableVariables.value.nodes[0]?.fields || []
      expect(updatedFields.some((f) => f.value !== undefined)).toBe(true)
    })
  })
})
