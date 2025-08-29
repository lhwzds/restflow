import { computed } from 'vue'
import { useExecutionStore } from '../../stores/executionStore'
import type { NodeExecutionStatus } from '../../stores/executionStore'

export function useNodeExecutionStatus() {
  const executionStore = useExecutionStore()

  /**
   * Get the execution status of a node
   */
  const getNodeStatus = (nodeId: string): NodeExecutionStatus | null => {
    return executionStore.getNodeStatus(nodeId)
  }

  /**
   * Get CSS classes for node based on execution status
   */
  const getNodeStatusClass = (nodeId: string): string => {
    const status = getNodeStatus(nodeId)
    if (!status) return ''
    
    return `execution-${status}`
  }

  /**
   * Check if node is currently executing
   */
  const isNodeExecuting = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Running'
  }

  /**
   * Check if node has error
   */
  const hasNodeError = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Failed'
  }

  /**
   * Check if node executed successfully
   */
  const hasNodeSuccess = (nodeId: string): boolean => {
    return getNodeStatus(nodeId) === 'Completed'
  }

  /**
   * Get node execution result
   */
  const getNodeResult = (nodeId: string) => {
    return executionStore.nodeResults.get(nodeId)
  }

  /**
   * Get a short preview of node output (for inline display)
   */
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

    // Truncate if too long
    if (preview.length > maxLength) {
      return preview.substring(0, maxLength) + '...'
    }
    
    return preview
  }

  /**
   * Get execution time for a node
   */
  const getNodeExecutionTime = (nodeId: string): number | null => {
    const result = getNodeResult(nodeId)
    return result?.executionTime || null
  }

  /**
   * Format execution time for display
   */
  const formatExecutionTime = (ms: number | null): string => {
    if (!ms) return ''
    if (ms < 1000) return `${ms}ms`
    return `${(ms / 1000).toFixed(2)}s`
  }

  /**
   * Get status icon for node
   */
  const getNodeStatusIcon = (nodeId: string): string => {
    const status = getNodeStatus(nodeId)
    switch (status) {
      case 'Completed':
        return '✅'
      case 'Failed':
        return '❌'
      case 'Running':
        return '⏳'
      case 'skipped':
        return '⏭️'
      default:
        return ''
    }
  }

  /**
   * Check if execution is currently running
   */
  const isExecuting = computed(() => executionStore.isExecuting)

  /**
   * Check if there are any execution results
   */
  const hasExecutionResults = computed(() => executionStore.hasResults)

  return {
    // Status checking
    getNodeStatus,
    getNodeStatusClass,
    isNodeExecuting,
    hasNodeError,
    hasNodeSuccess,
    
    // Result access
    getNodeResult,
    getNodeOutputPreview,
    getNodeExecutionTime,
    
    // Formatting
    formatExecutionTime,
    getNodeStatusIcon,
    
    // Global state
    isExecuting,
    hasExecutionResults,
  }
}