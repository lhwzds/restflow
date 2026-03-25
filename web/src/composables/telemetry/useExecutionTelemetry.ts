import { ref, watch, type Ref } from 'vue'
import {
  getRunExecutionMetrics,
  getRunExecutionTimeline,
  queryRunExecutionLogs,
} from '@/api/execution-traces'
import type { ExecutionLogResponse } from '@/types/generated/ExecutionLogResponse'
import type { ExecutionMetricsResponse } from '@/types/generated/ExecutionMetricsResponse'
import type { ExecutionTimeline } from '@/types/generated/ExecutionTimeline'

function toErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }
  return 'Unknown telemetry error'
}

export function useExecutionTelemetry(runId: Ref<string | null>) {
  const timeline = ref<ExecutionTimeline | null>(null)
  const metrics = ref<ExecutionMetricsResponse | null>(null)
  const logs = ref<ExecutionLogResponse | null>(null)

  const isLoadingTimeline = ref(false)
  const isLoadingMetrics = ref(false)
  const isLoadingLogs = ref(false)

  const timelineError = ref<string | null>(null)
  const metricsError = ref<string | null>(null)
  const logsError = ref<string | null>(null)

  let loadVersion = 0

  async function refresh() {
    const currentRunId = runId.value?.trim() || null
    const version = ++loadVersion

    if (!currentRunId) {
      timeline.value = null
      metrics.value = null
      logs.value = null
      timelineError.value = null
      metricsError.value = null
      logsError.value = null
      isLoadingTimeline.value = false
      isLoadingMetrics.value = false
      isLoadingLogs.value = false
      return
    }

    isLoadingTimeline.value = true
    isLoadingMetrics.value = true
    isLoadingLogs.value = true
    timelineError.value = null
    metricsError.value = null
    logsError.value = null

    const [timelineResult, metricsResult, logsResult] = await Promise.allSettled([
      getRunExecutionTimeline(currentRunId),
      getRunExecutionMetrics(currentRunId),
      queryRunExecutionLogs(currentRunId),
    ])

    if (version !== loadVersion) {
      return
    }

    if (timelineResult.status === 'fulfilled') {
      timeline.value = timelineResult.value
      timelineError.value = null
    } else {
      timeline.value = null
      timelineError.value = toErrorMessage(timelineResult.reason)
    }
    isLoadingTimeline.value = false

    if (metricsResult.status === 'fulfilled') {
      metrics.value = metricsResult.value
      metricsError.value = null
    } else {
      metrics.value = null
      metricsError.value = toErrorMessage(metricsResult.reason)
    }
    isLoadingMetrics.value = false

    if (logsResult.status === 'fulfilled') {
      logs.value = logsResult.value
      logsError.value = null
    } else {
      logs.value = null
      logsError.value = toErrorMessage(logsResult.reason)
    }
    isLoadingLogs.value = false
  }

  watch(
    runId,
    () => {
      void refresh()
    },
    { immediate: true },
  )

  return {
    timeline,
    metrics,
    logs,
    isLoadingTimeline,
    isLoadingMetrics,
    isLoadingLogs,
    timelineError,
    metricsError,
    logsError,
    refresh,
  }
}
