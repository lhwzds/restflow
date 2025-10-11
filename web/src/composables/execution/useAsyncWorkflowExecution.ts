import { ElMessage } from 'element-plus'
import { ref, onUnmounted } from 'vue'
import * as workflowsApi from '../../api/workflows'
import * as tasksApi from '../../api/tasks'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useExecutionStore } from '../../stores/executionStore'
import { useWorkflowPersistence } from '../persistence/useWorkflowPersistence'
import type { Task } from '@/types/generated/Task'
import { ERROR_MESSAGES, SUCCESS_MESSAGES, POLLING_TIMING, INFO_MESSAGES } from '@/constants'

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
    if (!workflowStore.currentWorkflowId) {
      const saveResult = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
        showMessage: false,
        meta: { name: workflowStore.currentWorkflowName || 'Untitled Workflow' }
      })
      
      if (!saveResult.success) {
        ElMessage.error(ERROR_MESSAGES.FAILED_TO_SAVE('workflow'))
        return { success: false, error: ERROR_MESSAGES.FAILED_TO_SAVE('workflow') }
      }
    }

    if (isExecuting.value) {
      ElMessage.warning(ERROR_MESSAGES.ALREADY_EXECUTING)
      return { success: false, error: ERROR_MESSAGES.ALREADY_EXECUTING }
    }

    isExecuting.value = true
    executionError.value = null

    try {
      const { execution_id } = await workflowsApi.executeAsyncSubmit(workflowStore.currentWorkflowId!)
      executionId.value = execution_id
      
      executionStore.startExecution(execution_id)
      
      startPolling()
      
      ElMessage.success(SUCCESS_MESSAGES.EXECUTED('Workflow'))
      return { success: true, executionId: execution_id }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED
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
    stopPolling()
    
    pollingInterval.value = window.setInterval(async () => {
      if (!executionId.value) {
        stopPolling()
        return
      }

      try {
        const tasks: Task[] = await tasksApi.getExecutionStatus(executionId.value)
        
        executionStore.updateFromTasks(tasks)
        
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
            ElMessage.success(SUCCESS_MESSAGES.EXECUTED('Workflow'))
          }
          
          executionStore.endExecution()
        }
      } catch (error) {
        console.error('Polling error:', error)
      }
    }, POLLING_TIMING.EXECUTION_STATUS)
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
    ElMessage.info(INFO_MESSAGES.EXECUTION_CANCELLED)
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
