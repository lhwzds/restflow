import type { ExecutionLogResponse } from '@/types/generated/ExecutionLogResponse'
import type { ExecutionMetricsResponse } from '@/types/generated/ExecutionMetricsResponse'
import type { ExecutionTimeline } from '@/types/generated/ExecutionTimeline'
import type { ExecutionTraceEvent } from '@/types/generated/ExecutionTraceEvent'
import type { ExecutionTraceQuery } from '@/types/generated/ExecutionTraceQuery'
import type { ProviderHealthQuery } from '@/types/generated/ProviderHealthQuery'
import type { ProviderHealthResponse } from '@/types/generated/ProviderHealthResponse'
import { requestOptional, requestTyped } from './http-client'

// Generic trace search is reserved for debug, search, and compatibility flows.
export async function queryExecutionTraces(
  query: ExecutionTraceQuery,
): Promise<ExecutionTraceEvent[]> {
  return requestTyped<ExecutionTraceEvent[]>({
    type: 'QueryExecutionTraces',
    data: { query },
  })
}

export async function queryRunExecutionTraces(
  runId: string,
  options?: Partial<Pick<ExecutionTraceQuery, 'category' | 'source' | 'limit' | 'offset'>>,
): Promise<ExecutionTraceEvent[]> {
  return queryExecutionTraces({
    task_id: null,
    run_id: runId,
    parent_run_id: null,
    session_id: null,
    turn_id: null,
    agent_id: null,
    category: options?.category ?? null,
    source: options?.source ?? null,
    from_timestamp: null,
    to_timestamp: null,
    limit: options?.limit ?? 200,
    offset: options?.offset ?? 0,
  })
}

export async function getRunExecutionTimeline(runId: string): Promise<ExecutionTimeline> {
  return requestTyped<ExecutionTimeline>({
    type: 'GetExecutionRunTimeline',
    data: { run_id: runId },
  })
}

export async function getRunExecutionMetrics(runId: string): Promise<ExecutionMetricsResponse> {
  return requestTyped<ExecutionMetricsResponse>({
    type: 'GetExecutionRunMetrics',
    data: { run_id: runId },
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

export async function queryRunExecutionLogs(runId: string): Promise<ExecutionLogResponse> {
  return requestTyped<ExecutionLogResponse>({
    type: 'QueryExecutionRunLogs',
    data: { run_id: runId },
  })
}

export async function getExecutionTraceById(id: string): Promise<ExecutionTraceEvent | null> {
  return requestOptional<ExecutionTraceEvent>({
    type: 'GetExecutionTraceById',
    data: { id },
  })
}
