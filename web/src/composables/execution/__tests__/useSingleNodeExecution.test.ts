import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useWorkflowStore } from '@/stores/workflowStore'
import { useExecutionStore } from '@/stores/executionStore'
import { useSingleNodeExecution } from '../useSingleNodeExecution'
import { NODE_TYPE } from '@/constants'
import type { Node } from '@vue-flow/core'
import * as tasksApi from '@/api/tasks'

// Mock API
vi.mock('@/api/tasks', () => ({
  testNodeExecution: vi.fn(),
}))

describe('useSingleNodeExecution', () => {
  let workflowStore: ReturnType<typeof useWorkflowStore>
  let executionStore: ReturnType<typeof useExecutionStore>

  beforeEach(() => {
    setActivePinia(createPinia())
    workflowStore = useWorkflowStore()
    executionStore = useExecutionStore()
    vi.clearAllMocks()
  })

  const createMockNode = (type: string, data: any): Node => ({
    id: `test-node-${Date.now()}`,
    type,
    position: { x: 0, y: 0 },
    data: {
      label: `Test ${type}`,
      ...data,
    },
  })

  describe('extractNodeConfig', () => {
    it('should extract Agent node config and remove label', () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'test prompt',
        temperature: 0.7,
        tools: ['calculator'],
        input: '{{input}}',
        api_key_config: { provider: 'openai' },
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()

      // Indirectly test extractNodeConfig through execution
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      executeSingleNode(node.id)

      expect(tasksApi.testNodeExecution).toHaveBeenCalledWith(
        expect.objectContaining({
          nodes: [
            {
              id: node.id,
              node_type: 'Agent',
              config: {
                type: NODE_TYPE.AGENT,
                data: {
                  model: 'gpt-4',
                  prompt: 'test prompt',
                  temperature: 0.7,
                  tools: ['calculator'],
                  input: '{{input}}',
                  api_key_config: { provider: 'openai' },
                },
              },
            },
          ],
        }),
      )

      // Verify that label is removed
      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config.data).not.toHaveProperty('label')
    })

    it('should extract HttpRequest node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.HTTP_REQUEST, {
        url: 'https://api.example.com',
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{"test": true}',
        auth: { type: 'bearer', token: 'secret' },
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      expect(tasksApi.testNodeExecution).toHaveBeenCalledWith(
        expect.objectContaining({
          nodes: [
            {
              id: node.id,
              node_type: 'HttpRequest',
              config: {
                type: NODE_TYPE.HTTP_REQUEST,
                data: {
                  url: 'https://api.example.com',
                  method: 'POST',
                  headers: { 'Content-Type': 'application/json' },
                  body: '{"test": true}',
                  auth: { type: 'bearer', token: 'secret' },
                },
              },
            },
          ],
        }),
      )
    })

    it('should extract Python node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.PYTHON, {
        code: 'print("hello world")',
        dependencies: ['requests', 'numpy'],
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config).toEqual({
        type: NODE_TYPE.PYTHON,
        data: expect.objectContaining({
          code: 'print("hello world")',
          dependencies: ['requests', 'numpy'],
        }),
      })
    })

    it('should extract Print node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.PRINT, {
        message: 'Test message',
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config.data).not.toHaveProperty('label')
    })

    it('should extract WebhookTrigger node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.WEBHOOK_TRIGGER, {
        path: '/webhook/test',
        method: 'POST',
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config).toEqual({
        type: NODE_TYPE.WEBHOOK_TRIGGER,
        data: {
          path: '/webhook/test',
          method: 'POST',
        },
      })
    })

    it('should extract ScheduleTrigger node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.SCHEDULE_TRIGGER, {
        cron: '0 * * * *',
        timezone: 'UTC',
        payload: { test: 'data' },
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config.data).not.toHaveProperty('label')
    })

    it('should extract ManualTrigger node config correctly', async () => {
      const node = createMockNode(NODE_TYPE.MANUAL_TRIGGER, {
        payload: null,
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      expect(callArgs.nodes[0]!.config).toEqual({
        type: NODE_TYPE.MANUAL_TRIGGER,
        data: expect.objectContaining({
          payload: null,
        }),
      })
    })

    it('should use fallback for unknown node types', async () => {
      const node = createMockNode('CustomNodeType', {
        customField: 'customValue',
        anotherField: 123,
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id)

      const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]
      // Unknown types should return full data (excluding label)
      expect(callArgs.nodes[0]!.config.data).toEqual({
        customField: 'customValue',
        anotherField: 123,
      })
    })
  })

  describe('executeSingleNode - format wrapping', () => {
    it('should wrap node config with {type, data} format for all node types', async () => {
      const testCases = [
        {
          type: NODE_TYPE.AGENT,
          data: { model: 'gpt-4', prompt: 'test' },
        },
        {
          type: NODE_TYPE.HTTP_REQUEST,
          data: { url: 'https://test.com', method: 'GET' },
        },
        {
          type: NODE_TYPE.PYTHON,
          data: { code: 'print("test")' },
        },
        {
          type: NODE_TYPE.PRINT,
          data: { message: 'test' },
        },
        {
          type: NODE_TYPE.WEBHOOK_TRIGGER,
          data: { path: '/webhook', method: 'POST' },
        },
        {
          type: NODE_TYPE.SCHEDULE_TRIGGER,
          data: { cron: '0 * * * *' },
        },
        {
          type: NODE_TYPE.MANUAL_TRIGGER,
          data: {},
        },
      ]

      for (const testCase of testCases) {
        const node = createMockNode(testCase.type, testCase.data)
        workflowStore.nodes = [node]

        const { executeSingleNode } = useSingleNodeExecution()
        vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

        await executeSingleNode(node.id)

        const callArgs = vi.mocked(tasksApi.testNodeExecution).mock.calls[0]![0]

        // Verify format wrapping
        expect(callArgs.nodes[0]!.config).toHaveProperty('type', testCase.type)
        expect(callArgs.nodes[0]!.config).toHaveProperty('data')
        expect(callArgs.nodes[0]!.config.data).toMatchObject(testCase.data)

        vi.clearAllMocks()
      }
    })

    it('should create correct test request structure', async () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'test',
      })

      workflowStore.nodes = [node]

      const { executeSingleNode } = useSingleNodeExecution()
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue({ result: 'success' })

      await executeSingleNode(node.id, { customInput: 'value' })

      expect(tasksApi.testNodeExecution).toHaveBeenCalledWith({
        id: expect.stringMatching(/^test-\d+$/),
        name: 'Test Agent Node',
        nodes: [
          {
            id: node.id,
            node_type: 'Agent',
            config: {
              type: NODE_TYPE.AGENT,
              data: {
                model: 'gpt-4',
                prompt: 'test',
              },
            },
          },
        ],
        edges: [],
        input: { customInput: 'value' },
      })
    })
  })

  describe('executeSingleNode - execution and store updates', () => {
    it('should update execution store on success', async () => {
      const node = createMockNode(NODE_TYPE.PRINT, {
        message: 'test',
      })

      workflowStore.nodes = [node]

      const mockResult = { printed: 'test message' }
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue(mockResult)

      const { executeSingleNode } = useSingleNodeExecution()

      const result = await executeSingleNode(node.id)

      expect(result).toEqual(mockResult)

      // Verify store updates
      const nodeResult = executionStore.getNodeResult(node.id)
      expect(nodeResult).toBeDefined()
      expect(nodeResult?.status).toBe('Completed')
      expect(nodeResult?.output).toEqual(mockResult)
      expect(nodeResult?.executionTime).toBeGreaterThanOrEqual(0)
    })

    it('should update execution store on failure', async () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'test',
      })

      workflowStore.nodes = [node]

      const mockError = {
        response: {
          data: {
            message: 'Execution failed: Model not available',
          },
        },
      }

      vi.mocked(tasksApi.testNodeExecution).mockRejectedValue(mockError)

      const { executeSingleNode, executionError } = useSingleNodeExecution()

      await expect(executeSingleNode(node.id)).rejects.toThrow(
        'Execution failed: Model not available',
      )

      // Verify error storage
      const nodeResult = executionStore.getNodeResult(node.id)
      expect(nodeResult?.status).toBe('Failed')
      expect(nodeResult?.error).toContain('Execution failed')
      expect(executionError.value).toContain('Execution failed')
    })

    it('should handle network errors gracefully', async () => {
      const node = createMockNode(NODE_TYPE.HTTP_REQUEST, {
        url: 'https://test.com',
        method: 'GET',
      })

      workflowStore.nodes = [node]

      vi.mocked(tasksApi.testNodeExecution).mockRejectedValue(new Error('Network Error'))

      const { executeSingleNode } = useSingleNodeExecution()

      await expect(executeSingleNode(node.id)).rejects.toThrow()

      const nodeResult = executionStore.getNodeResult(node.id)
      expect(nodeResult?.status).toBe('Failed')
    })

    it('should record execution time', async () => {
      const node = createMockNode(NODE_TYPE.PRINT, { message: 'test' })
      workflowStore.nodes = [node]

      vi.mocked(tasksApi.testNodeExecution).mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve({ result: 'ok' }), 50)),
      )

      const { executeSingleNode } = useSingleNodeExecution()

      await executeSingleNode(node.id)

      const nodeResult = executionStore.getNodeResult(node.id)
      expect(nodeResult?.executionTime).toBeGreaterThanOrEqual(50)
    })

    it('should update node data with last execution result', async () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'test',
      })

      workflowStore.nodes = [node]

      const mockResult = { response: 'AI response' }
      vi.mocked(tasksApi.testNodeExecution).mockResolvedValue(mockResult)

      const { executeSingleNode } = useSingleNodeExecution()

      await executeSingleNode(node.id, { input: 'test' })

      const updatedNode = workflowStore.nodes.find((n) => n.id === node.id)
      expect(updatedNode?.data.lastExecutionResult).toEqual(mockResult)
      expect(updatedNode?.data.lastExecutionInput).toEqual({ input: 'test' })
      expect(updatedNode?.data.lastExecutionTime).toBeDefined()
    })
  })

  describe('validateNodeConfig', () => {
    it('should validate Agent node requires model', async () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        prompt: 'test prompt',
        // missing model
      })

      workflowStore.nodes = [node]

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      expect(result.valid).toBe(false)
      expect(result.errors).toContain('Please select an AI model')
    })

    it('should validate HttpRequest node requires url', async () => {
      const node = createMockNode(NODE_TYPE.HTTP_REQUEST, {
        method: 'GET',
        // missing url
      })

      workflowStore.nodes = [node]

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      expect(result.valid).toBe(false)
      expect(result.errors).toContain('Please enter request URL')
    })

    it('should validate Python node requires code', async () => {
      const node = createMockNode(NODE_TYPE.PYTHON, {
        dependencies: [],
        // missing code
      })

      workflowStore.nodes = [node]

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      expect(result.valid).toBe(false)
      expect(result.errors.length).toBeGreaterThan(0)
    })

    it('should pass validation for valid Agent node', async () => {
      const sourceNode = createMockNode(NODE_TYPE.MANUAL_TRIGGER, {})
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'valid prompt',
      })

      workflowStore.nodes = [sourceNode, node]
      workflowStore.edges = [
        {
          id: 'e1',
          source: sourceNode.id,
          target: node.id,
        },
      ]

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      expect(result.valid).toBe(true)
      expect(result.errors).toHaveLength(0)
    })

    it('should pass validation for valid HttpRequest node', async () => {
      const sourceNode = createMockNode(NODE_TYPE.MANUAL_TRIGGER, {})
      const node = createMockNode(NODE_TYPE.HTTP_REQUEST, {
        url: 'https://api.example.com',
        method: 'GET',
      })

      workflowStore.nodes = [sourceNode, node]
      workflowStore.edges = [
        {
          id: 'e1',
          source: sourceNode.id,
          target: node.id,
        },
      ]

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      expect(result.valid).toBe(true)
      expect(result.errors).toHaveLength(0)
    })

    it('should warn for non-trigger nodes without inputs', async () => {
      const node = createMockNode(NODE_TYPE.AGENT, {
        model: 'gpt-4',
        prompt: 'test',
      })

      workflowStore.nodes = [node]
      workflowStore.edges = [] // no edges

      const { validateNodeConfig } = useSingleNodeExecution()
      const result = await validateNodeConfig(node.id)

      // Non-trigger nodes without input edges should return validation error
      expect(result.valid).toBe(false)
      expect(result.errors).toContain('Node requires input connection')
    })
  })
})
