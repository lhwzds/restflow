import { ElMessage } from 'element-plus'
import { ref } from 'vue'
import { workflowService } from '../../services/workflowService'
import { useWorkflowStore } from '../../stores/workflowStore'

export function useWorkflowExecution() {
  const workflowStore = useWorkflowStore()
  const isExecuting = ref(false)
  const executionResult = ref<any>(null)
  const executionError = ref<string | null>(null)

  /**
   * Execute current workflow
   */
  const executeCurrentWorkflow = async () => {
    // Check if can execute
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

    // Update store state
    workflowStore.setExecutionState(true, null, null)

    try {
      const result = await workflowService.execute({
        nodes: workflowStore.nodes,
        edges: workflowStore.edges,
        meta: { name: 'Current Workflow' }
      })

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
   * Execute workflow by ID
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
      const result = await workflowService.execute(workflowId)
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

  /**
   * Clear execution results
   */
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