import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionContext } from '@/types/generated/ExecutionContext'
import { API_ENDPOINTS } from '@/constants'

export const listWorkflows = async (): Promise<Workflow[]> => {
  if (isTauri()) {
    return invokeCommand<Workflow[]>('list_workflows')
  }
  const response = await apiClient.get<{ status: string; data: Workflow[] }>(API_ENDPOINTS.WORKFLOW.LIST)
  return response.data.data
}

export const createWorkflow = async (workflow: Workflow): Promise<{ id: string }> => {
  if (isTauri()) {
    const result = await invokeCommand<Workflow>('create_workflow', { workflow })
    return { id: result.id }
  }
  const response = await apiClient.post<{
    status: string
    message: string
    data: { id: string }
  }>(API_ENDPOINTS.WORKFLOW.CREATE, workflow)
  return response.data.data
}

export const getWorkflow = async (id: string): Promise<Workflow> => {
  if (isTauri()) {
    return invokeCommand<Workflow>('get_workflow', { id })
  }
  const response = await apiClient.get<Workflow>(API_ENDPOINTS.WORKFLOW.GET(id))
  return response.data
}

export const updateWorkflow = async (id: string, workflow: Workflow): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('update_workflow', { id, workflow })
    return
  }
  await apiClient.put<{
    status: string
    message: string
  }>(API_ENDPOINTS.WORKFLOW.UPDATE(id), workflow)
}

export const deleteWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('delete_workflow', { id })
    return
  }
  await apiClient.delete<{
    status: string
    message: string
  }>(API_ENDPOINTS.WORKFLOW.DELETE(id))
}

export const executeSyncRun = async (workflow: Workflow): Promise<ExecutionContext> => {
  if (isTauri()) {
    // For inline workflow execution, we need to create it first in Tauri mode
    const { id } = await createWorkflow(workflow)
    return invokeCommand<ExecutionContext>('execute_workflow_sync', { 
      workflow_id: id, 
      input: {} 
    })
  }
  const response = await apiClient.post<{
    status: string
    data: ExecutionContext
  }>(API_ENDPOINTS.EXECUTION.SYNC_RUN, workflow)
  return response.data.data
}

export const executeSyncRunById = async (id: string, input: any = {}): Promise<ExecutionContext> => {
  if (isTauri()) {
    return invokeCommand<ExecutionContext>('execute_workflow_sync', { 
      workflow_id: id, 
      input 
    })
  }
  const response = await apiClient.post<{
    status: string
    data: ExecutionContext
  }>(API_ENDPOINTS.EXECUTION.SYNC_RUN_BY_ID(id), { input })
  return response.data.data
}

export const executeAsyncSubmit = async (
  id: string,
  initialVariables?: any
): Promise<{ execution_id: string }> => {
  if (isTauri()) {
    const taskId = await invokeCommand<string>('submit_workflow', { 
      workflow_id: id, 
      input: initialVariables || {} 
    })
    return { execution_id: taskId }
  }
  const response = await apiClient.post<{
    status: string
    data: { execution_id: string }
  }>(API_ENDPOINTS.EXECUTION.ASYNC_SUBMIT(id), { initial_variables: initialVariables })
  return response.data.data
}

