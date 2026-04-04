import type { ExecutionContainerSummary } from '@/types/generated/ExecutionContainerSummary'
import type { ChildRunListQuery } from '@/types/generated/ChildRunListQuery'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import type { RunListQuery } from '@/types/generated/RunListQuery'
import type { RunSummary } from '@/types/generated/RunSummary'
import { requestTyped } from './http-client'

export type { ChildRunListQuery, RunListQuery, RunSummary }

export async function listExecutionContainers(): Promise<ExecutionContainerSummary[]> {
  return requestTyped<ExecutionContainerSummary[]>({
    type: 'ListExecutionContainers',
  })
}

export async function listRuns(query: RunListQuery): Promise<RunSummary[]> {
  return requestTyped<RunSummary[]>({
    type: 'ListRuns',
    data: { query },
  })
}

export async function getExecutionRunThread(runId: string): Promise<ExecutionThread> {
  return requestTyped<ExecutionThread>({
    type: 'GetExecutionRunThread',
    data: { run_id: runId },
  })
}

export async function listChildRuns(
  query: ChildRunListQuery,
): Promise<RunSummary[]> {
  return requestTyped<RunSummary[]>({
    type: 'ListChildRuns',
    data: { query },
  })
}
