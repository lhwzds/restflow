import { computed } from 'vue'
import { useExecutionStore } from '../../stores/executionStore'
import type { NodeExecutionStatus } from '../../stores/executionStore'

export function useNodeExecutionStatus() {
  const executionStore = useExecutionStore()

  const getNodeStatus = (nodeId: string): NodeExecutionStatus | null => {
    return executionStore.getNodeStatus(nodeId)
  }

  const getNodeStatusClass = (nodeId: string): string => {
    const status = getNodeStatus(nodeId)
    if (!status) return ''
    return `execution-${status.toLowerCase()}`
  }

  const isNodeExecuting = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Running'
  }

  const hasNodeError = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Failed'
  }

  const hasNodeSuccess = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Completed'
  }

  const getNodeResult = (nodeId: string) => {
    return executionStore.nodeResults.get(nodeId)
  }

  const getNodeOutputPreview = (nodeId: string, maxLength = 50): string | null => {
    const result = getNodeResult(nodeId)
    if (!result?.output) return null

    let preview = ''

    if (typeof result.output === 'string') {
      preview = result.output
    } else if (typeof result.output === 'object') {
      try {
        preview = JSON.stringify(result.output)
      } catch {
        preview = '[Object]'
      }
    } else {
      preview = String(result.output)
    }

    if (preview.length > maxLength) {
      return preview.substring(0, maxLength) + '...'
    }

    return preview
  }

  const getNodeExecutionTime = (nodeId: string): number | null => {
    const result = getNodeResult(nodeId)
    return result?.executionTime || null
  }

  const formatExecutionTime = (ms: number | null): string => {
    if (!ms) return ''
    if (ms < 1000) return `${ms}ms`
    return `${(ms / 1000).toFixed(2)}s`
  }

  const isExecuting = computed(() => executionStore.isExecuting)

  const hasExecutionResults = computed(() => executionStore.hasResults)

  return {
    getNodeStatus,
    getNodeStatusClass,
    isNodeExecuting,
    hasNodeError,
    hasNodeSuccess,
    getNodeResult,
    getNodeOutputPreview,
    getNodeExecutionTime,
    formatExecutionTime,
    isExecuting,
    hasExecutionResults,
  }
}
