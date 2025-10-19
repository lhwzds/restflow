import { ref, onUnmounted, computed, watch, type Ref } from 'vue'
import { ElMessage } from 'element-plus'
import * as workflowsApi from '../../api/workflows'
import * as tasksApi from '../../api/tasks'
import { useExecutionStore } from '../../stores/executionStore'
import type { ExecutionSummary } from '@/types/generated/ExecutionSummary'
import type { ExecutionStatus } from '@/types/generated/ExecutionStatus'
import { POLLING_TIMING, ERROR_MESSAGES } from '@/constants'

export function useExecutionHistory(workflowId: Readonly<Ref<string | null | undefined>>) {
  const executionStore = useExecutionStore()

  const executions = ref<ExecutionSummary[]>([])
  const isLoading = ref(false)
  const pollingInterval = ref<number | null>(null)
  const selectedExecutionId = ref<string | null>(null)

  const loadHistory = async () => {
    const currentWorkflowId = workflowId.value
    if (!currentWorkflowId) return

    isLoading.value = true
    try {
      const history = await workflowsApi.listWorkflowExecutions(currentWorkflowId, 20)
      executions.value = history
    } catch (error) {
      console.error('Failed to load execution history:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_LOAD('execution history'))
    } finally {
      isLoading.value = false
    }
  }

  const startPolling = () => {
    const currentWorkflowId = workflowId.value
    if (!currentWorkflowId) return

    stopPolling()
    loadHistory()

    pollingInterval.value = window.setInterval(async () => {
      const previousCount = executions.value.length
      await loadHistory()

      if (executions.value.length > previousCount && executions.value[0]) {
        const newExecution = executions.value[0]
        ElMessage.info(`New execution detected: ${newExecution.execution_id}`)
      }
    }, POLLING_TIMING.EXECUTION_HISTORY || 5000)
  }

  const stopPolling = () => {
    if (pollingInterval.value) {
      clearInterval(pollingInterval.value)
      pollingInterval.value = null
    }
  }

  const switchToExecution = async (executionId: string) => {
    selectedExecutionId.value = executionId
    executionStore.startExecution(executionId)

    try {
      const tasks = await tasksApi.getExecutionStatus(executionId)
      executionStore.updateFromTasks(tasks)

      const allCompleted = tasks.every(t => t.status === 'Completed' || t.status === 'Failed')
      if (allCompleted) {
        executionStore.endExecution()
      }
    } catch (error) {
      console.error('Failed to load execution details:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_LOAD('execution details'))
    }
  }

  const getStatusText = (status: ExecutionStatus): string => {
    const statusMap: Record<ExecutionStatus, string> = {
      Running: 'Running',
      Completed: 'Completed',
      Failed: 'Failed'
    }
    return statusMap[status] || status
  }

  const getStatusIcon = (status: ExecutionStatus): string => {
    const iconMap: Record<ExecutionStatus, string> = {
      Running: '⏳',
      Completed: '✓',
      Failed: '✗'
    }
    return iconMap[status] || '?'
  }

  const formatRelativeTime = (timestamp: number): string => {
    const now = Date.now()
    const diff = now - timestamp

    const seconds = Math.floor(diff / 1000)
    const minutes = Math.floor(seconds / 60)
    const hours = Math.floor(minutes / 60)
    const days = Math.floor(hours / 24)

    if (seconds < 60) return 'just now'
    if (minutes < 60) return `${minutes} min ago`
    if (hours < 24) return `${hours} hr ago`
    if (days < 7) return `${days} days ago`

    const date = new Date(timestamp)
    return date.toLocaleDateString('en-US')
  }

  const hasRunningExecution = computed(() => {
    return executions.value.some(e => e.status === 'Running')
  })

  const latestExecution = computed(() => {
    return executions.value[0] || null
  })

  onUnmounted(() => {
    stopPolling()
  })

  watch(
    workflowId,
    newId => {
      if (!newId) {
        stopPolling()
        executions.value = []
        selectedExecutionId.value = null
        return
      }

      executions.value = []
      selectedExecutionId.value = null
      startPolling()
    }
  )

  return {
    // State
    executions,
    isLoading,
    selectedExecutionId,
    hasRunningExecution,
    latestExecution,

    // Methods
    loadHistory,
    startPolling,
    stopPolling,
    switchToExecution,
    getStatusText,
    getStatusIcon,
    formatRelativeTime,
  }
}
