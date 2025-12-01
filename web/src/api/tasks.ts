import { apiClient } from './config'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'
import { API_ENDPOINTS } from '@/constants'

interface NodeTestRequest {
  nodes: Array<{
    id: string
    node_type: string
    config: Record<string, unknown>
  }>
  edges: Array<Record<string, unknown>>
  input: unknown
}

export const getTaskStatus = async (
  id: string,
): Promise<{
  id: string
  status: TaskStatus
  result?: any
  error?: string
}> => {
  const response = await apiClient.get<Task>(API_ENDPOINTS.TASK.STATUS(id))
  const task = response.data
  return {
    id: task.id,
    status: task.status,
    result: task.output,
    error: task.error || undefined,
  }
}

export const listTasks = async (params?: {
  execution_id?: string
  workflow_id?: string
  status?: TaskStatus
  limit?: number
  offset?: number
}): Promise<Task[]> => {
  const response = await apiClient.get<Task[]>(API_ENDPOINTS.TASK.LIST, { params })
  return response.data
}

export const getTasksByExecutionId = async (executionId: string): Promise<Task[]> => {
  return listTasks({ execution_id: executionId })
}

export const getExecutionStatus = async (executionId: string): Promise<Task[]> => {
  const response = await apiClient.get<Task[]>(API_ENDPOINTS.EXECUTION.STATUS(executionId))
  return response.data
}

export const testNodeExecution = async <T = any>(payload: NodeTestRequest): Promise<T> => {
  const response = await apiClient.post<T>(API_ENDPOINTS.EXECUTION.INLINE_RUN, payload)
  return response.data
}

export const executeNode = async (node: any, _input: any = {}): Promise<string> => {
  const response = await apiClient.post<{ task_id: string; message: string }>(
    API_ENDPOINTS.NODE.EXECUTE,
    node,
  )
  return response.data.task_id
}
