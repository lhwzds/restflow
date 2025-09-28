import { ref } from 'vue'
import { useNodeOperations } from '@/composables/node/useNodeOperations'
import { testNodeExecution } from '@/api/tasks'
import type { Node } from '@vue-flow/core'
import { NODE_TYPE, TRIGGER_NODE_TYPES, ERROR_MESSAGES, VALIDATION_MESSAGES } from '@/constants'

export function useSingleNodeExecution() {
  const { getNodeById, getIncomingEdges, updateNodeData } = useNodeOperations()

  const isExecuting = ref(false)
  const executionResult = ref<any>(null)
  const executionError = ref<string | null>(null)

  const executeSingleNode = async (nodeId: string, testInput?: any) => {
    isExecuting.value = true
    executionResult.value = null
    executionError.value = null

    try {
      const node = getNodeById(nodeId)
      if (!node) {
        throw new Error(ERROR_MESSAGES.NOT_FOUND('Node'))
      }

      const nodeType = node.type ?? NODE_TYPE.MANUAL_TRIGGER

      let input = testInput
      if (!input) {
        const incomingEdges = getIncomingEdges(nodeId)
        if (incomingEdges.length > 0) {
          const sourceNodeId = incomingEdges[0].source
          const sourceNode = getNodeById(sourceNodeId)
          if (sourceNode?.data?.lastExecutionResult) {
            input = sourceNode.data.lastExecutionResult
          }
        }
      }

      const testRequest = {
        nodes: [{
          id: node.id,
          node_type: mapNodeTypeToBackend(nodeType),
          config: extractNodeConfig(node)
        }],
        edges: [],
        input: input || {}
      }

      const result = await testNodeExecution<any>(testRequest)
      executionResult.value = result

      updateNodeData(nodeId, {
        lastExecutionInput: input || {},
        lastExecutionResult: executionResult.value,
        lastExecutionTime: new Date().toISOString()
      })

      return executionResult.value
    } catch (error: any) {
      const errorMessage =
        error?.response?.data?.error ||
        error?.response?.data?.message ||
        error?.message ||
        ERROR_MESSAGES.NODE_EXECUTION_FAILED

      executionError.value = errorMessage
      throw new Error(errorMessage)
    } finally {
      isExecuting.value = false
    }
  }

  const executeMultipleNodes = async (nodeIds: string[]) => {
    const results: Record<string, any> = {}
    const errors: Record<string, string> = {}

    for (const nodeId of nodeIds) {
      try {
        const result = await executeSingleNode(nodeId)
        results[nodeId] = result
      } catch (error: any) {
        errors[nodeId] = error.message
      }
    }

    return { results, errors }
  }

  const validateNodeConfig = async (nodeId: string) => {
    const node = getNodeById(nodeId)
    if (!node) {
      return { valid: false, errors: [ERROR_MESSAGES.NOT_FOUND('Node')] }
    }

    const errors: string[] = []

    switch (node.type) {
      case NODE_TYPE.AGENT:
        if (!node.data.model) {
          errors.push(VALIDATION_MESSAGES.SELECT_MODEL)
        }
        if (!node.data.prompt && !node.data.input) {
          errors.push(VALIDATION_MESSAGES.ENTER_PROMPT)
        }
        break

      case NODE_TYPE.HTTP_REQUEST:
        if (!node.data.url) {
          errors.push(VALIDATION_MESSAGES.ENTER_URL)
        }
        if (!node.data.method) {
          errors.push(VALIDATION_MESSAGES.REQUIRED_SELECT('request method'))
        }
        break

      case NODE_TYPE.WEBHOOK_TRIGGER:
        if (!node.data.path) {
          errors.push(VALIDATION_MESSAGES.SET_WEBHOOK_PATH)
        }
        break
    }

    if (!TRIGGER_NODE_TYPES.has(node.type as any)) {
      const incomingEdges = getIncomingEdges(nodeId)
      if (incomingEdges.length === 0) {
        errors.push(ERROR_MESSAGES.NODE_INPUT_REQUIRED)
      }
    }

    return {
      valid: errors.length === 0,
      errors
    }
  }

  const getMockInput = (nodeId: string) => {
    const node = getNodeById(nodeId)
    if (!node) return {}

    switch (node.type) {
      case 'agentNode':
        return {
          message: 'This is a test message',
          context: {
            user: 'test_user',
            session: 'test_session'
          }
        }

      case 'httpNode':
        return {
          data: {
            test: true,
            timestamp: new Date().toISOString()
          }
        }

      default:
        return {
          test: true,
          value: 'mock_value'
        }
    }
  }

  return {
    isExecuting,
    executionResult,
    executionError,
    executeSingleNode,
    executeMultipleNodes,
    validateNodeConfig,
    getMockInput
  }
}

function mapNodeTypeToBackend(nodeType: string): string {
  const validTypes = Object.values(NODE_TYPE)
  if (validTypes.includes(nodeType as any)) {
    return nodeType
  }

  return nodeType
}

function extractNodeConfig(node: Node): any {
  const { label, ...config } = node.data

  switch (node.type) {
    case NODE_TYPE.AGENT:
      return {
        model: config.model,
        prompt: config.prompt,
        temperature: config.temperature,
        tools: config.tools,
        input: config.input,
        api_key_config: config.api_key_config
      }

    case NODE_TYPE.HTTP_REQUEST:
      return {
        url: config.url,
        method: config.method,
        headers: config.headers,
        body: config.body,
        auth: config.auth
      }

    case NODE_TYPE.WEBHOOK_TRIGGER:
      return {
        path: config.path,
        method: config.method || 'POST'
      }

    case NODE_TYPE.MANUAL_TRIGGER:
      return config

    default:
      return config
  }
}
