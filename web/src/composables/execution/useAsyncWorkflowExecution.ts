import { ElMessage } from 'element-plus'
import { onUnmounted, ref } from 'vue'
import * as tasksApi from '../../api/tasks'
import * as workflowsApi from '../../api/workflows'
import { useExecutionStore } from '../../stores/executionStore'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useWorkflowPersistence } from '../persistence/useWorkflowPersistence'
import type { Task } from '@/types/generated/Task'
import { ERROR_MESSAGES, SUCCESS_MESSAGES, POLLING_TIMING, INFO_MESSAGES } from '@/constants'

interface MonitorExecutionOptions {
  label?: string
  startPolling?: boolean
  notifyQueued?: boolean
  queuedMessage?: string
}

const formatQueuedMessage = (label: string) => `${label} execution started`

function createExecutionMonitor() {
  const executionStore = useExecutionStore()

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
          (task: Task) => task.status === 'Completed' || task.status === 'Failed',
        )
        const hasFailed = tasks.some((task: Task) => task.status === 'Failed')

        if (allCompleted) {
          stopPolling()
          isExecuting.value = false

          if (hasFailed) {
            const failedTasks = tasks.filter((task: Task) => task.status === 'Failed')
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

  const monitorExecution = (id: string, options: MonitorExecutionOptions = {}) => {
    const {
      label = 'Workflow',
      startPolling: shouldStartPolling = true,
      notifyQueued = true,
      queuedMessage,
    } = options

    stopPolling()
    executionId.value = id
    executionLabel.value = label
    executionError.value = null
    isExecuting.value = true

    executionStore.startExecution(id)

    if (shouldStartPolling) {
      startPolling()
    }

    if (notifyQueued) {
      const message = queuedMessage ?? formatQueuedMessage(label)
      ElMessage.success(message)
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
    monitorExecution,
    cancelExecution,
    clearExecutionResults,
    startPolling,
    stopPolling,
  }
}

type ExecutionMonitor = ReturnType<typeof createExecutionMonitor>

let monitor: ExecutionMonitor | null = null
let subscribers = 0

export function useExecutionMonitor() {
  if (!monitor) {
    monitor = createExecutionMonitor()
  }

  subscribers += 1

  onUnmounted(() => {
    subscribers = Math.max(0, subscribers - 1)
    if (subscribers === 0) {
      monitor?.stopPolling()
    }
  })

  return {
    isExecuting: monitor.isExecuting,
    executionId: monitor.executionId,
    executionError: monitor.executionError,
    monitorExecution: monitor.monitorExecution,
    cancelExecution: monitor.cancelExecution,
    clearExecutionResults: monitor.clearExecutionResults,
    startPolling: monitor.startPolling,
    stopPolling: monitor.stopPolling,
  }
}

export function useAsyncWorkflowExecution() {
  const workflowStore = useWorkflowStore()
  const { saveWorkflow } = useWorkflowPersistence()
  const executionMonitor = useExecutionMonitor()

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

    if (executionMonitor.isExecuting.value) {
      ElMessage.warning(ERROR_MESSAGES.ALREADY_EXECUTING)
      return { success: false, error: ERROR_MESSAGES.ALREADY_EXECUTING }
    }

    executionMonitor.isExecuting.value = true
    executionMonitor.executionError.value = null

    try {
      const { execution_id } = await workflowsApi.submitWorkflow(workflowStore.currentWorkflowId!)
      executionMonitor.monitorExecution(execution_id, {
        label: 'Workflow',
      })
      return { success: true, executionId: execution_id }
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED
      executionMonitor.executionError.value = errorMessage
      executionMonitor.isExecuting.value = false

      ElMessage.error(`Execution failed: ${errorMessage}`)
      return { success: false, error: errorMessage }
    }
  }

  return {
    isExecuting: executionMonitor.isExecuting,
    executionId: executionMonitor.executionId,
    executionError: executionMonitor.executionError,
    startAsyncExecution,
    cancelExecution: executionMonitor.cancelExecution,
    clearExecutionResults: executionMonitor.clearExecutionResults,
    monitorExecution: executionMonitor.monitorExecution,
  }
}
