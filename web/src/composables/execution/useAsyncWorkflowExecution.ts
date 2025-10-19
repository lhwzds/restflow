import { ElMessage } from 'element-plus'
import { onUnmounted, ref } from 'vue'
import * as workflowsApi from '../../api/workflows'
import * as tasksApi from '../../api/tasks'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useExecutionStore } from '../../stores/executionStore'
import { useWorkflowPersistence } from '../persistence/useWorkflowPersistence'
import type { Task } from '@/types/generated/Task'
import { ERROR_MESSAGES, SUCCESS_MESSAGES, POLLING_TIMING, INFO_MESSAGES } from '@/constants'

function createAsyncExecutionManager() {
  const workflowStore = useWorkflowStore()
  const executionStore = useExecutionStore()
  const { saveWorkflow } = useWorkflowPersistence()

  const isExecuting = ref(false)
  const executionId = ref<string | null>(null)
  const pollingInterval = ref<number | null>(null)
  const executionError = ref<string | null>(null)
  const executionLabel = ref<string>('Workflow')

  const stopPolling = () => {
    if (pollingInterval.value) {
      clearInterval(pollingInterval.value)
      pollingInterval.value = null
    }
  }

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

        const allCompleted = tasks.every(
          (t: Task) => t.status === 'Completed' || t.status === 'Failed'
        )
        const hasFailed = tasks.some((t: Task) => t.status === 'Failed')

        if (allCompleted) {
          stopPolling()
          isExecuting.value = false

          if (hasFailed) {
            const failedTasks = tasks.filter((t: Task) => t.status === 'Failed')
            const errorMsg = failedTasks[0]?.error || 'Unknown error'
            ElMessage.error(`${executionLabel.value} execution failed: ${errorMsg}`)
            executionError.value = errorMsg
          } else {
            ElMessage.success(SUCCESS_MESSAGES.EXECUTED(executionLabel.value))
          }

          executionStore.endExecution()
        }
      } catch (error) {
        console.error('Polling error:', error)
      }
    }, POLLING_TIMING.EXECUTION_STATUS)
  }

  const monitorExecution = (
    id: string,
    options: { label?: string; queuedMessage?: string } = {}
  ) => {
    stopPolling()
    executionId.value = id
    executionLabel.value = options.label ?? 'Workflow'
    executionError.value = null
    isExecuting.value = true

    executionStore.startExecution(id)
    startPolling()

    if (options.queuedMessage) {
      ElMessage.success(options.queuedMessage)
    }
  }

  const startAsyncExecution = async () => {
    if (!workflowStore.currentWorkflowId) {
      const saveResult = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
        showMessage: false,
        meta: { name: workflowStore.currentWorkflowName || 'Untitled Workflow' },
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
      const { execution_id } = await workflowsApi.executeAsyncSubmit(
        workflowStore.currentWorkflowId!
      )
      monitorExecution(execution_id, {
        label: 'Workflow',
        queuedMessage: SUCCESS_MESSAGES.EXECUTED('Workflow'),
      })
      return { success: true, executionId: execution_id }
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED
      executionError.value = errorMessage
      isExecuting.value = false

      ElMessage.error(`Execution failed: ${errorMessage}`)
      return { success: false, error: errorMessage }
    }
  }

  const cancelExecution = () => {
    stopPolling()
    isExecuting.value = false
    executionId.value = null
    ElMessage.info(INFO_MESSAGES.EXECUTION_CANCELLED)
  }

  const clearExecutionResults = () => {
    stopPolling()
    executionId.value = null
    executionError.value = null
    isExecuting.value = false
    executionStore.clearExecution()
  }

  return {
    isExecuting,
    executionId,
    executionError,
    startAsyncExecution,
    cancelExecution,
    clearExecutionResults,
    monitorExecution,
    stopPolling,
  }
}

type AsyncExecutionManager = ReturnType<typeof createAsyncExecutionManager>

let manager: AsyncExecutionManager | null = null
let subscribers = 0

export function useAsyncWorkflowExecution() {
  if (!manager) {
    manager = createAsyncExecutionManager()
  }

  subscribers += 1

  onUnmounted(() => {
    subscribers = Math.max(0, subscribers - 1)
    if (subscribers === 0) {
      manager?.stopPolling()
    }
  })

  return {
    isExecuting: manager.isExecuting,
    executionId: manager.executionId,
    executionError: manager.executionError,
    startAsyncExecution: manager.startAsyncExecution,
    cancelExecution: manager.cancelExecution,
    clearExecutionResults: manager.clearExecutionResults,
    monitorExecution: manager.monitorExecution,
  }
}
