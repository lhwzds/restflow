import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
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

export const getTaskStatus = async (id: string): Promise<{
  id: string
  status: TaskStatus
  result?: any
  error?: string
}> => {
  if (isTauri()) {
    const task = await invokeCommand<Task>('get_task_status', { task_id: id })
    return {
      id: task.id,
      status: task.status,
      result: task.output,
      error: task.error || undefined
    }
  }
  const response = await apiClient.get<Task>(API_ENDPOINTS.TASK.STATUS(id))
  const task = response.data
  return {
    id: task.id,
    status: task.status,
    result: task.output,
    error: task.error || undefined
  }
}

export const listTasks = async (params?: {
  execution_id?: string
  workflow_id?: string
  status?: TaskStatus
  limit?: number
  offset?: number
}): Promise<Task[]> => {
  if (isTauri()) {
    return invokeCommand<Task[]>('list_tasks', {
      execution_id: params?.execution_id,
      status: params?.status,
      limit: params?.limit || 100
    })
  }
  const response = await apiClient.get<Task[]>(API_ENDPOINTS.TASK.LIST, { params })
  return response.data
}

export const getTasksByExecutionId = async (executionId: string): Promise<Task[]> => {
  return listTasks({ execution_id: executionId })
}

export const getExecutionStatus = async (executionId: string): Promise<Task[]> => {
  if (isTauri()) {
    return invokeCommand<Task[]>('get_execution_status', {
      execution_id: executionId
    })
  }
  const response = await apiClient.get<Task[]>(API_ENDPOINTS.EXECUTION.STATUS(executionId))
  return response.data
}

export const testNodeExecution = async <T = any>(payload: NodeTestRequest): Promise<T> => {
  if (isTauri()) {
    throw new Error('Node testing is not supported in desktop mode yet')
  }

  const response = await apiClient.post<T>(API_ENDPOINTS.EXECUTION.SYNC_RUN, payload)

  return response.data
}

export const executeNode = async (node: any, input: any = {}): Promise<string> => {
  if (isTauri()) {
    return invokeCommand<string>('execute_node', {
      node,
      input
    })
  }
  const response = await apiClient.post<{ task_id: string; message: string }>(
    API_ENDPOINTS.NODE.EXECUTE,
    node
  )
  return response.data.task_id
}
