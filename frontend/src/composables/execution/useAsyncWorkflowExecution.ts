import { ElMessage } from 'element-plus'
import { ref, onUnmounted } from 'vue'
import * as workflowsApi from '../../api/workflows'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useExecutionStore } from '../../stores/executionStore'
import { useWorkflowPersistence } from '../persistence/useWorkflowPersistence'
import type { Task } from '@/types/generated/Task'

export function useAsyncWorkflowExecution() {
  const workflowStore = useWorkflowStore()
  const executionStore = useExecutionStore()
  const { saveWorkflow } = useWorkflowPersistence()
  
  const isExecuting = ref(false)
  const executionId = ref<string | null>(null)
  const pollingInterval = ref<number | null>(null)
  const executionError = ref<string | null>(null)

  /**
   * Start async execution
   */
  const startAsyncExecution = async () => {
    // Auto-save workflow if not saved
    if (!workflowStore.currentWorkflowId) {
      const saveResult = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
        showMessage: false,
        meta: { name: workflowStore.currentWorkflowName || 'Untitled Workflow' }
      })
      
      if (!saveResult.success) {
        ElMessage.error('Failed to save workflow')
        return { success: false, error: 'Failed to save workflow' }
      }
    }

    if (isExecuting.value) {
      ElMessage.warning('Already executing')
      return { success: false, error: 'Already executing' }
    }

    isExecuting.value = true
    executionError.value = null

    try {
      // Submit async execution (workflowStore.currentWorkflowId is guaranteed to exist here)
      const { execution_id } = await workflowsApi.executeAsyncSubmit(workflowStore.currentWorkflowId!)
      executionId.value = execution_id
      
      // Start execution in store
      executionStore.startExecution(execution_id)
      
      // Start polling for status immediately
      startPolling()
      
      ElMessage.success('Workflow execution started')
      return { success: true, executionId: execution_id }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to start execution'
      executionError.value = errorMessage
      isExecuting.value = false
      
      ElMessage.error(`Execution failed: ${errorMessage}`)
      return { success: false, error: errorMessage }
    }
  }

  /**
   * Poll execution status
   */
  const startPolling = () => {
    // Clear any existing polling
    stopPolling()
    
    // Poll every 500ms for real-time feel
    pollingInterval.value = window.setInterval(async () => {
      if (!executionId.value) {
        stopPolling()
        return
      }

      try {
        // API returns an array of tasks for the execution
        const tasks: Task[] = await workflowsApi.getExecutionStatus(executionId.value)
        
        // Update all tasks at once using the new method
        executionStore.updateFromTasks(tasks)
        
        // Check if all tasks are complete
        const allCompleted = tasks.every((t: Task) => t.status === 'Completed' || t.status === 'Failed')
        const hasFailed = tasks.some((t: Task) => t.status === 'Failed')
        
        if (allCompleted) {
          stopPolling()
          isExecuting.value = false
          
          if (hasFailed) {
            const failedTasks = tasks.filter((t: Task) => t.status === 'Failed')
            const errorMsg = failedTasks[0]?.error || 'Unknown error'
            ElMessage.error(`Workflow execution failed: ${errorMsg}`)
            executionError.value = errorMsg
          } else {
            ElMessage.success('Workflow execution completed')
          }
          
          executionStore.endExecution()
        }
      } catch (error) {
        console.error('Polling error:', error)
        // Continue polling even on error
      }
    }, 500)
  }

  /**
   * Stop polling
   */
  const stopPolling = () => {
    if (pollingInterval.value) {
      clearInterval(pollingInterval.value)
      pollingInterval.value = null
    }
  }

  /**
   * Cancel execution
   */
  const cancelExecution = () => {
    stopPolling()
    isExecuting.value = false
    executionId.value = null
    ElMessage.info('Execution cancelled')
  }

  /**
   * Clear execution results
   */
  const clearExecutionResults = () => {
    stopPolling()
    executionId.value = null
    executionError.value = null
    isExecuting.value = false
    executionStore.clearExecution()
  }

  // Cleanup on unmount
  onUnmounted(() => {
    stopPolling()
  })

  return {
    // State
    isExecuting,
    executionId,
    executionError,

    // Methods
    startAsyncExecution,
    cancelExecution,
    clearExecutionResults,
  }
}