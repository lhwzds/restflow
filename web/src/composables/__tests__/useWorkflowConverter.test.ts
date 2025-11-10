import { describe, it, expect } from 'vitest'
import { useWorkflowConverter } from '@/composables/editor/useWorkflowConverter'
import type { Workflow } from '@/types/generated/Workflow'
import type { Node as VueFlowNode, Edge as VueFlowEdge } from '@vue-flow/core'

describe('useWorkflowConverter', () => {
  const { convertFromBackendFormat, convertToBackendFormat } = useWorkflowConverter()

  describe('convertFromBackendFormat', () => {
    it('should extract nested data for WebhookTrigger nodes', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'webhook-1',
            node_type: 'WebhookTrigger',
            config: {
              type: 'WebhookTrigger',
              data: {
                path: '/api/webhook/test',
                method: 'POST',
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({
        path: '/api/webhook/test',
        method: 'POST',
      })
      expect(result.nodes[0]!.data).not.toHaveProperty('type')
    })

    it('should extract nested data for ScheduleTrigger nodes', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'schedule-1',
            node_type: 'ScheduleTrigger',
            config: {
              type: 'ScheduleTrigger',
              data: {
                cron: '0 0 * * * *',
                timezone: 'UTC',
                payload: { test: true },
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({
        cron: '0 0 * * * *',
        timezone: 'UTC',
        payload: { test: true },
      })
    })

    it('should extract nested data for ManualTrigger nodes', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'manual-1',
            node_type: 'ManualTrigger',
            config: {
              type: 'ManualTrigger',
              data: {
                payload: { message: 'User triggered' },
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({
        payload: { message: 'User triggered' },
      })
    })

    it('should require new format with type and data fields', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'webhook-1',
            node_type: 'WebhookTrigger',
            config: {
              type: 'WebhookTrigger', // Must have type
              data: {
                // Must have data
                path: '/api/webhook',
                method: 'POST',
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({
        path: '/api/webhook',
        method: 'POST',
      })
    })

    it('should handle old format by using entire config as fallback', () => {
      // Frontend still handles old format gracefully for backward compatibility
      // (only in frontend display, backend will reject old format)
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'webhook-1',
            node_type: 'WebhookTrigger',
            config: {
              path: '/api/webhook/old', // Old format without type/data wrapper
              method: 'GET',
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      // Frontend converter falls back to using entire config when no 'data' field
      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({
        path: '/api/webhook/old',
        method: 'GET',
      })
    })

    it('should keep non-trigger nodes unchanged', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'agent-1',
            node_type: 'Agent',
            config: {
              type: 'Agent',
              data: {
                model: 'gpt-4',
                prompt: 'Test prompt',
              },
            },
            position: { x: 100, y: 100 },
          },
          {
            id: 'http-1',
            node_type: 'HttpRequest',
            config: {
              type: 'HttpRequest',
              data: {
                url: 'https://api.example.com',
                method: 'GET',
              },
            },
            position: { x: 200, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(2)
      // Non-trigger nodes extract the data portion (flattened for frontend)
      expect(result.nodes[0]!.data).toEqual({
        model: 'gpt-4',
        prompt: 'Test prompt',
      })
      expect(result.nodes[1]!.data).toEqual({
        url: 'https://api.example.com',
        method: 'GET',
      })
    })

    it('should handle empty config', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'webhook-1',
            node_type: 'WebhookTrigger',
            config: {},
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.data).toEqual({})
    })

    it('should convert edges correctly', () => {
      const workflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [],
        edges: [
          { from: 'node1', to: 'node2' },
          { from: 'node2', to: 'node3' },
        ],
      }

      const result = convertFromBackendFormat(workflow)

      expect(result.edges).toHaveLength(2)
      expect(result.edges[0]!).toEqual({
        id: 'enode1-node2',
        source: 'node1',
        target: 'node2',
        animated: true,
      })
      expect(result.edges[1]!).toEqual({
        id: 'enode2-node3',
        source: 'node2',
        target: 'node3',
        animated: true,
      })
    })
  })

  describe('convertToBackendFormat', () => {
    it('should wrap WebhookTrigger data with type', () => {
      const nodes: VueFlowNode[] = [
        {
          id: 'webhook-1',
          type: 'WebhookTrigger',
          position: { x: 100, y: 100 },
          data: {
            path: '/api/webhook/test',
            method: 'POST',
          },
        },
      ]
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.config).toEqual({
        type: 'WebhookTrigger',
        data: {
          path: '/api/webhook/test',
          method: 'POST',
        },
      })
    })

    it('should wrap ScheduleTrigger data with type', () => {
      const nodes: VueFlowNode[] = [
        {
          id: 'schedule-1',
          type: 'ScheduleTrigger',
          position: { x: 100, y: 100 },
          data: {
            cron: '0 0 * * * *',
            timezone: 'UTC',
          },
        },
      ]
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.config).toEqual({
        type: 'ScheduleTrigger',
        data: {
          cron: '0 0 * * * *',
          timezone: 'UTC',
        },
      })
    })

    it('should wrap ManualTrigger data with type', () => {
      const nodes: VueFlowNode[] = [
        {
          id: 'manual-1',
          type: 'ManualTrigger',
          position: { x: 100, y: 100 },
          data: {
            payload: { test: true },
          },
        },
      ]
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.config).toEqual({
        type: 'ManualTrigger',
        data: {
          payload: { test: true },
        },
      })
    })

    it('should keep non-trigger nodes unchanged', () => {
      const nodes: VueFlowNode[] = [
        {
          id: 'agent-1',
          type: 'Agent',
          position: { x: 100, y: 100 },
          data: {
            model: 'gpt-4',
            prompt: 'Test',
          },
        },
        {
          id: 'http-1',
          type: 'HttpRequest',
          position: { x: 200, y: 100 },
          data: {
            url: 'https://api.example.com',
          },
        },
      ]
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.nodes).toHaveLength(2)
      // Non-trigger nodes are wrapped with {type, data} format for backend
      expect(result.nodes[0]!.config).toEqual({
        type: 'Agent',
        data: {
          model: 'gpt-4',
          prompt: 'Test',
        },
      })
      expect(result.nodes[1]!.config).toEqual({
        type: 'HttpRequest',
        data: {
          url: 'https://api.example.com',
        },
      })
    })

    it('should handle empty data', () => {
      const nodes: VueFlowNode[] = [
        {
          id: 'webhook-1',
          type: 'WebhookTrigger',
          position: { x: 100, y: 100 },
          data: {},
        },
      ]
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.nodes).toHaveLength(1)
      expect(result.nodes[0]!.config).toEqual({
        type: 'WebhookTrigger',
        data: {},
      })
    })

    it('should convert edges correctly', () => {
      const nodes: VueFlowNode[] = []
      const edges: VueFlowEdge[] = [
        { id: 'e1', source: 'node1', target: 'node2' },
        { id: 'e2', source: 'node2', target: 'node3' },
      ]

      const result = convertToBackendFormat(nodes, edges)

      expect(result.edges).toHaveLength(2)
      expect(result.edges[0]!).toEqual({ from: 'node1', to: 'node2' })
      expect(result.edges[1]!).toEqual({ from: 'node2', to: 'node3' })
    })

    it('should use provided workflow metadata', () => {
      const nodes: VueFlowNode[] = []
      const edges: VueFlowEdge[] = []
      const meta = {
        id: 'custom-id',
        name: 'Custom Workflow',
      }

      const result = convertToBackendFormat(nodes, edges, meta)

      expect(result.id).toBe('custom-id')
      expect(result.name).toBe('Custom Workflow')
    })

    it('should generate default metadata when not provided', () => {
      const nodes: VueFlowNode[] = []
      const edges: VueFlowEdge[] = []

      const result = convertToBackendFormat(nodes, edges)

      expect(result.id).toMatch(/^workflow-\d+$/)
      expect(result.name).toBe('My Workflow')
    })
  })

  describe('round-trip conversion', () => {
    it('should preserve trigger config through round-trip conversion', () => {
      const originalWorkflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'webhook-1',
            node_type: 'WebhookTrigger',
            config: {
              type: 'WebhookTrigger',
              data: {
                path: '/api/webhook/test',
                method: 'POST',
                auth: {
                  type: 'ApiKey',
                  key: 'test-key',
                },
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [{ from: 'webhook-1', to: 'agent-1' }],
      }

      const { nodes, edges } = convertFromBackendFormat(originalWorkflow)
      const converted = convertToBackendFormat(nodes, edges, {
        id: originalWorkflow.id,
        name: originalWorkflow.name,
      })

      expect(converted.nodes[0]!.config).toEqual(originalWorkflow.nodes[0]!.config)
      expect(converted.edges).toEqual(originalWorkflow.edges)
    })

    it('should preserve non-trigger config through round-trip conversion', () => {
      const originalWorkflow: Workflow = {
        id: 'test-workflow',
        name: 'Test Workflow',
        nodes: [
          {
            id: 'agent-1',
            node_type: 'Agent',
            config: {
              type: 'Agent',
              data: {
                model: 'gpt-4',
                prompt: 'Test prompt',
                temperature: 0.7,
              },
            },
            position: { x: 100, y: 100 },
          },
        ],
        edges: [],
      }

      const { nodes, edges } = convertFromBackendFormat(originalWorkflow)
      const converted = convertToBackendFormat(nodes, edges, {
        id: originalWorkflow.id,
        name: originalWorkflow.name,
      })

      expect(converted.nodes[0]!.config).toEqual(originalWorkflow.nodes[0]!.config)
    })
  })
})
