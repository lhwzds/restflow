import type { ExecutionContainerSummary } from '@/types/generated/ExecutionContainerSummary'
import type { ChildExecutionSessionQuery } from '@/types/generated/ChildExecutionSessionQuery'
import type { ExecutionSessionListQuery } from '@/types/generated/ExecutionSessionListQuery'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import { requestTyped } from './http-client'

export async function listExecutionContainers(): Promise<ExecutionContainerSummary[]> {
  return requestTyped<ExecutionContainerSummary[]>({
    type: 'ListExecutionContainers',
  })
}

export async function listExecutionSessions(
  query: ExecutionSessionListQuery,
): Promise<ExecutionSessionSummary[]> {
  return requestTyped<ExecutionSessionSummary[]>({
    type: 'ListExecutionSessions',
    data: { query },
  })
}

export async function getExecutionRunThread(runId: string): Promise<ExecutionThread> {
  return requestTyped<ExecutionThread>({
    type: 'GetExecutionRunThread',
    data: { run_id: runId },
  })
}

export async function listChildExecutionSessions(
  query: ChildExecutionSessionQuery,
): Promise<ExecutionSessionSummary[]> {
  return requestTyped<ExecutionSessionSummary[]>({
    type: 'ListChildExecutionSessions',
    data: { query },
  })
}
