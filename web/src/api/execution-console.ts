import type { ChildExecutionSessionQuery } from '@/types/generated/ChildExecutionSessionQuery'
import type { ExecutionSessionListQuery } from '@/types/generated/ExecutionSessionListQuery'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import type { ExecutionThreadQuery } from '@/types/generated/ExecutionThreadQuery'
import { requestTyped } from './http-client'

export async function listExecutionSessions(
  query: ExecutionSessionListQuery,
): Promise<ExecutionSessionSummary[]> {
  return requestTyped<ExecutionSessionSummary[]>({
    type: 'ListExecutionSessions',
    data: { query },
  })
}

export async function getExecutionThread(query: ExecutionThreadQuery): Promise<ExecutionThread> {
  return requestTyped<ExecutionThread>({
    type: 'GetExecutionThread',
    data: { query },
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
