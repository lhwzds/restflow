import type { ExecutionLogQuery } from '@/types/generated/ExecutionLogQuery'
import type { ExecutionLogResponse } from '@/types/generated/ExecutionLogResponse'
import type { ExecutionMetricQuery } from '@/types/generated/ExecutionMetricQuery'
import type { ExecutionMetricsResponse } from '@/types/generated/ExecutionMetricsResponse'
import type { ExecutionTimeline } from '@/types/generated/ExecutionTimeline'
import type { ExecutionTraceEvent } from '@/types/generated/ExecutionTraceEvent'
import type { ExecutionTraceQuery } from '@/types/generated/ExecutionTraceQuery'
import type { ExecutionTraceStats } from '@/types/generated/ExecutionTraceStats'
import type { ProviderHealthQuery } from '@/types/generated/ProviderHealthQuery'
import type { ProviderHealthResponse } from '@/types/generated/ProviderHealthResponse'
import { requestOptional, requestTyped } from './http-client'

export async function queryExecutionTraces(
  query: ExecutionTraceQuery,
): Promise<ExecutionTraceEvent[]> {
  return requestTyped<ExecutionTraceEvent[]>({
    type: 'QueryExecutionTraces',
    data: { query },
  })
}

export async function getExecutionTimeline(query: ExecutionTraceQuery): Promise<ExecutionTimeline> {
  return requestTyped<ExecutionTimeline>({
    type: 'GetExecutionTimeline',
    data: { query },
  })
}

export async function getExecutionMetrics(
  query: ExecutionMetricQuery,
): Promise<ExecutionMetricsResponse> {
  return requestTyped<ExecutionMetricsResponse>({
    type: 'GetExecutionMetrics',
    data: { query },
  })
}

export async function getProviderHealth(
  query: ProviderHealthQuery,
): Promise<ProviderHealthResponse> {
  return requestTyped<ProviderHealthResponse>({
    type: 'GetProviderHealth',
    data: { query },
  })
}

export async function queryExecutionLogs(query: ExecutionLogQuery): Promise<ExecutionLogResponse> {
  return requestTyped<ExecutionLogResponse>({
    type: 'QueryExecutionLogs',
    data: { query },
  })
}

export async function getExecutionTraceStats(taskId?: string): Promise<ExecutionTraceStats> {
  return requestTyped<ExecutionTraceStats>({
    type: 'GetExecutionTraceStats',
    data: { task_id: taskId ?? null },
  })
}

export async function getExecutionTraceById(id: string): Promise<ExecutionTraceEvent | null> {
  return requestOptional<ExecutionTraceEvent>({
    type: 'GetExecutionTraceById',
    data: { id },
  })
}
