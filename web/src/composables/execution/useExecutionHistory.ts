import { ref, onUnmounted, computed, watch, type Ref } from 'vue'
import { ElMessage } from 'element-plus'
import * as workflowsApi from '../../api/workflows'
import * as tasksApi from '../../api/tasks'
import { useExecutionStore } from '../../stores/executionStore'
import type { ExecutionSummary } from '@/types/generated/ExecutionSummary'
import type { ExecutionHistoryPage } from '@/types/generated/ExecutionHistoryPage'
import type { ExecutionStatus } from '@/types/generated/ExecutionStatus'
import { POLLING_TIMING, ERROR_MESSAGES } from '@/constants'

export function useExecutionHistory(workflowId: Readonly<Ref<string | null | undefined>>) {
  const executionStore = useExecutionStore()

  const executions = ref<ExecutionSummary[]>([])
  const isLoading = ref(false)
  const pollingInterval = ref<number | null>(null)
  const selectedExecutionId = ref<string | null>(null)
  const isFirstPoll = ref(true)
  const page = ref(1)
  const pageSize = ref(20)
  const totalExecutions = ref(0)
  const totalPages = ref(0)

  const hasPrevPage = computed(() => page.value > 1)
  const hasNextPage = computed(() => {
    if (totalPages.value === 0) return false
    return page.value < totalPages.value
  })

  const loadHistory = async (targetPage?: number) => {
    const currentWorkflowId = workflowId.value
    if (!currentWorkflowId) return null

    isLoading.value = true
    const requestedPage = targetPage ?? page.value
    const previousFirstId = executions.value[0]?.execution_id ?? null
    try {
      const history: ExecutionHistoryPage = await workflowsApi.listWorkflowExecutions(
        currentWorkflowId,
        requestedPage,
        pageSize.value
      )

      const newItems = history.items
      const newFirstId = newItems[0]?.execution_id ?? null

      executions.value = newItems
      totalExecutions.value = history.total
      page.value = history.page
      pageSize.value = history.page_size
      totalPages.value = history.total_pages

      return { previousFirstId, newFirstId }
    } catch (error) {
      console.error('Failed to load execution history:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_LOAD('execution history'))
      return null
    } finally {
      isLoading.value = false
    }
  }

  const startPolling = () => {
    const currentWorkflowId = workflowId.value
    if (!currentWorkflowId) return

    stopPolling()
    isFirstPoll.value = true
    loadHistory(page.value)

    pollingInterval.value = window.setInterval(async () => {
      const result = await loadHistory(page.value)

      if (!isFirstPoll.value && page.value === 1 && result?.newFirstId && result.previousFirstId !== result.newFirstId) {
        ElMessage.info(`New execution detected: ${result.newFirstId}`)
      }
      isFirstPoll.value = false
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

      // Rollback execution state on error
      executionStore.clearExecution()
      selectedExecutionId.value = null
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
    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric'
    })
  }

  const formatFullDateTime = (timestamp: number): string => {
    const date = new Date(timestamp)
    return date.toLocaleString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: true
    })
  }

  const hasRunningExecution = computed(() => {
    return executions.value.some(e => e.status === 'Running')
  })

  const latestExecution = computed(() => {
    return executions.value[0] || null
  })

  const goToPage = async (targetPage: number) => {
    if (targetPage < 1) return
    if (totalPages.value > 0 && targetPage > totalPages.value) return
    await loadHistory(targetPage)
  }

  const goToNextPage = async () => {
    if (hasNextPage.value) {
      await goToPage(page.value + 1)
    }
  }

  const goToPrevPage = async () => {
    if (hasPrevPage.value) {
      await goToPage(page.value - 1)
    }
  }

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
      page.value = 1
      totalExecutions.value = 0
      totalPages.value = 0
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
    totalExecutions,
    page,
    pageSize,
    totalPages,
    hasNextPage,
    hasPrevPage,

    // Methods
    loadHistory,
    startPolling,
    stopPolling,
    switchToExecution,
    getStatusText,
    getStatusIcon,
    formatRelativeTime,
    formatFullDateTime,
    goToPage,
    goToNextPage,
    goToPrevPage,
  }
}
