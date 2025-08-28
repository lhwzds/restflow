import { apiClient } from './config'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'

// GET /api/task/status/{id}
export const getTaskStatus = async (id: string): Promise<{
  id: string
  status: TaskStatus
  result?: any
  error?: string
}> => {
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

// GET /api/task/list
export const listTasks = async (params?: {
  execution_id?: string
  workflow_id?: string
  status?: TaskStatus
  limit?: number
  offset?: number
}): Promise<Task[]> => {
  const response = await apiClient.get<{
    status: string
    data: Task[]
  }>('/api/task/list', { params })
  return response.data.data
}

// GET /api/task/execution/{execution_id}
export const getTasksByExecutionId = async (executionId: string): Promise<Task[]> => {
  const response = await apiClient.get<{
    status: string
    data: Task[]
  }>(`/api/task/execution/${executionId}`)
  return response.data.data
}

// POST /api/task/retry/{id}
export const retryTask = async (id: string): Promise<void> => {
  await apiClient.post<{
    status: string
    message: string
  }>(`/api/task/retry/${id}`)
}

// POST /api/task/cancel/{id}
export const cancelTask = async (id: string): Promise<void> => {
  await apiClient.post<{
    status: string
    message: string
  }>(`/api/task/cancel/${id}`)
}