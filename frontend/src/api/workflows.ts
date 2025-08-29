import { apiClient } from './config'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionContext } from '@/types/generated/ExecutionContext'
import type { Task } from '@/types/generated/Task'

export const listWorkflows = async (): Promise<Workflow[]> => {
  const response = await apiClient.get<{ status: string; data: Workflow[] }>('/api/workflow/list')
  return response.data.data
}

export const createWorkflow = async (workflow: Workflow): Promise<{ id: string }> => {
  const response = await apiClient.post<{
    status: string
    message: string
    data: { id: string }
  }>('/api/workflow/create', workflow)
  return response.data.data
}

export const getWorkflow = async (id: string): Promise<Workflow> => {
  const response = await apiClient.get<Workflow>(`/api/workflow/get/${id}`)
  return response.data
}

export const updateWorkflow = async (id: string, workflow: Workflow): Promise<void> => {
  await apiClient.put<{
    status: string
    message: string
  }>(`/api/workflow/update/${id}`, workflow)
}

export const deleteWorkflow = async (id: string): Promise<void> => {
  await apiClient.delete<{
    status: string
    message: string
  }>(`/api/workflow/delete/${id}`)
}

export const executeSyncRun = async (workflow: Workflow): Promise<ExecutionContext> => {
  const response = await apiClient.post<{
    status: string
    data: ExecutionContext
  }>('/api/execution/sync/run', workflow)
  return response.data.data
}

export const executeSyncRunById = async (id: string): Promise<ExecutionContext> => {
  const response = await apiClient.post<{
    status: string
    data: ExecutionContext
  }>(`/api/execution/sync/run-workflow/${id}`)
  return response.data.data
}

export const executeAsyncSubmit = async (
  id: string,
  initialVariables?: any
): Promise<{ execution_id: string }> => {
  const response = await apiClient.post<{
    status: string
    data: { execution_id: string }
  }>(`/api/execution/async/submit/${id}`, { initial_variables: initialVariables })
  return response.data.data
}

export const getExecutionStatus = async (id: string): Promise<Task[]> => {
  const response = await apiClient.get<{
    status: string
    data: Task[]
  }>(`/api/execution/status/${id}`)
  return response.data.data
}