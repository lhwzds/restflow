import { apiClient } from './config'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionContext } from '@/types/generated/ExecutionContext'
import type { ExecutionHistoryPage } from '@/types/generated/ExecutionHistoryPage'
import { API_ENDPOINTS } from '@/constants'

export const listWorkflows = async (): Promise<Workflow[]> => {
  const response = await apiClient.get<Workflow[]>(API_ENDPOINTS.WORKFLOW.LIST)
  return response.data
}

export const createWorkflow = async (workflow: Workflow): Promise<{ id: string }> => {
  const response = await apiClient.post<{ id: string }>(API_ENDPOINTS.WORKFLOW.CREATE, workflow)
  return response.data
}

export const getWorkflow = async (id: string): Promise<Workflow> => {
  const response = await apiClient.get<Workflow>(API_ENDPOINTS.WORKFLOW.GET(id))
  return response.data
}

export const updateWorkflow = async (id: string, workflow: Workflow): Promise<void> => {
  await apiClient.put<void>(API_ENDPOINTS.WORKFLOW.UPDATE(id), workflow)
}

export const deleteWorkflow = async (id: string): Promise<void> => {
  await apiClient.delete<void>(API_ENDPOINTS.WORKFLOW.DELETE(id))
}

export const executeInline = async (workflow: Workflow): Promise<ExecutionContext> => {
  const response = await apiClient.post<ExecutionContext>(
    API_ENDPOINTS.EXECUTION.INLINE_RUN,
    workflow,
  )
  return response.data
}

export const submitWorkflow = async (
  id: string,
  initialVariables?: any,
): Promise<{ execution_id: string }> => {
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
  const response = await apiClient.get<ExecutionHistoryPage>(API_ENDPOINTS.EXECUTION.HISTORY(id), {
    params: { page, page_size: pageSize },
  })
  return response.data
}
