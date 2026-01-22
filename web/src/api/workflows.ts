import { apiClient, isTauri, tauriInvoke } from './config'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionHistoryPage } from '@/types/generated/ExecutionHistoryPage'
import { API_ENDPOINTS } from '@/constants'

export const listWorkflows = async (): Promise<Workflow[]> => {
  if (isTauri()) {
    return tauriInvoke<Workflow[]>('list_workflows')
  }
  const response = await apiClient.get<Workflow[]>(API_ENDPOINTS.WORKFLOW.LIST)
  return response.data
}

export const createWorkflow = async (workflow: Workflow): Promise<{ id: string }> => {
  if (isTauri()) {
    const result = await tauriInvoke<Workflow>('create_workflow', { workflow })
    return { id: result.id }
  }
  const response = await apiClient.post<{ id: string }>(API_ENDPOINTS.WORKFLOW.CREATE, workflow)
  return response.data
}

export const getWorkflow = async (id: string): Promise<Workflow> => {
  if (isTauri()) {
    return tauriInvoke<Workflow>('get_workflow', { id })
  }
  const response = await apiClient.get<Workflow>(API_ENDPOINTS.WORKFLOW.GET(id))
  return response.data
}

export const updateWorkflow = async (id: string, workflow: Workflow): Promise<void> => {
  if (isTauri()) {
    await tauriInvoke<Workflow>('update_workflow', { id, workflow })
    return
  }
  await apiClient.put<void>(API_ENDPOINTS.WORKFLOW.UPDATE(id), workflow)
}

export const deleteWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    return tauriInvoke<void>('delete_workflow', { id })
  }
  await apiClient.delete<void>(API_ENDPOINTS.WORKFLOW.DELETE(id))
}

export const submitWorkflow = async (
  id: string,
  initialVariables?: unknown,
): Promise<{ execution_id: string }> => {
  if (isTauri()) {
    const executionId = await tauriInvoke<string>('execute_workflow', {
      id,
      input: initialVariables ?? null,
    })
    return { execution_id: executionId }
  }
  const response = await apiClient.post<{ execution_id: string; workflow_id: string }>(
    API_ENDPOINTS.EXECUTION.SUBMIT(id),
    { initial_variables: initialVariables },
  )
  return { execution_id: response.data.execution_id }
}

export const listWorkflowExecutions = async (
  id: string,
  page = 1,
  pageSize = 20,
): Promise<ExecutionHistoryPage> => {
  if (isTauri()) {
    return tauriInvoke<ExecutionHistoryPage>('get_workflow_executions', {
      workflow_id: id,
      page,
      page_size: pageSize,
    })
  }
  const response = await apiClient.get<ExecutionHistoryPage>(API_ENDPOINTS.EXECUTION.HISTORY(id), {
    params: { page, page_size: pageSize },
  })
  return response.data
}
