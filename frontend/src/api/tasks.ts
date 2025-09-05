import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'

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
  const response = await apiClient.get<{
    status: string
    data: {
      id: string
      status: TaskStatus
      result?: any
      error?: string
    }
  }>(`/api/task/status/${id}`)
  return response.data.data
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
  const response = await apiClient.get<{
    status: string
    data: Task[]
  }>('/api/task/list', { params })
  return response.data.data
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
  const response = await apiClient.get<{
    status: string
    data: Task[]
  }>(`/api/execution/status/${executionId}`)
  return response.data.data
}

export const executeNode = async (node: any, input: any = {}): Promise<string> => {
  if (isTauri()) {
    return invokeCommand<string>('execute_node', {
      node,
      input
    })
  }
  const response = await apiClient.post<{
    status: string
    data: { task_id: string }
  }>('/api/node/execute', node)
  return response.data.data.task_id
}