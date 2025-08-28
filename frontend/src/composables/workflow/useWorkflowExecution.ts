import { ElMessage } from 'element-plus'
import { ref } from 'vue'
import * as workflowsApi from '../../api/workflows'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useWorkflowConverter } from './useWorkflowConverter'

export function useWorkflowExecution() {
  const workflowStore = useWorkflowStore()
  const isExecuting = ref(false)
  const executionResult = ref<any>(null)
  const executionError = ref<string | null>(null)

  /**
   * Execute current workflow
   */
  const executeCurrentWorkflow = async () => {
    if (!workflowStore.nodes.length) {
      const error = 'No nodes to execute'
      ElMessage.error(error)
      return { success: false, error }
    }

    if (isExecuting.value) {
      const error = 'Already executing'
      ElMessage.warning(error)
      return { success: false, error }
    }

    isExecuting.value = true
    executionResult.value = null
    executionError.value = null

    workflowStore.setExecutionState(true, null, null)

    try {
      const { convertToBackendFormat } = useWorkflowConverter()
      const workflow = convertToBackendFormat(
        workflowStore.nodes,
        workflowStore.edges,
        { name: 'Current Workflow' }
      )
      const result = await workflowsApi.executeSyncRun(workflow)

      executionResult.value = result
      workflowStore.setExecutionState(false, result, null)

      ElMessage.success('Workflow executed successfully')
      console.log('Workflow executed:', result)

      return { success: true, data: result }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error occurred'
      executionError.value = errorMessage
      workflowStore.setExecutionState(false, null, errorMessage)

      ElMessage.error(`Execution failed: ${errorMessage}`)
      console.error('Workflow execution failed:', error)

      return { success: false, error: errorMessage }
    } finally {
      isExecuting.value = false
    }
  }

  /**
   * Execute workflow by ID (sync execution)
   */
  const executeWorkflowById = async (workflowId: string) => {
    if (!workflowId) {
      const error = 'Invalid workflow ID'
      ElMessage.error(error)
      return { success: false, error }
    }

    isExecuting.value = true
    executionResult.value = null
    executionError.value = null

    try {
      const result = await workflowsApi.executeSyncRunById(workflowId)
      executionResult.value = result

      ElMessage.success('Workflow executed successfully')
      return { success: true, data: result }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error occurred'
      executionError.value = errorMessage

      ElMessage.error(`Execution failed: ${errorMessage}`)
      console.error('Workflow execution failed:', error)

      return { success: false, error: errorMessage }
    } finally {
      isExecuting.value = false
    }
  }

  const clearExecutionResults = () => {
    executionResult.value = null
    executionError.value = null
    workflowStore.clearExecutionState()
  }

  return {
    // State
    isExecuting,
    executionResult,
    executionError,

    // Methods
    executeCurrentWorkflow,
    executeWorkflowById,
    clearExecutionResults,
  }
}
