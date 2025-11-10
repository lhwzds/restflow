import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
import type { Workflow } from '@/types/generated/Workflow'
import type { ExecutionContext } from '@/types/generated/ExecutionContext'
import type { ExecutionSummary } from '@/types/generated/ExecutionSummary'
import type { ExecutionHistoryPage } from '@/types/generated/ExecutionHistoryPage'
import { API_ENDPOINTS } from '@/constants'

export const listWorkflows = async (): Promise<Workflow[]> => {
  if (isTauri()) {
    return invokeCommand<Workflow[]>('list_workflows')
  }
  const response = await apiClient.get<Workflow[]>(API_ENDPOINTS.WORKFLOW.LIST)
  return response.data
}

export const createWorkflow = async (workflow: Workflow): Promise<{ id: string }> => {
  if (isTauri()) {
    const result = await invokeCommand<Workflow>('create_workflow', { workflow })
    return { id: result.id }
  }
  const response = await apiClient.post<{ id: string }>(API_ENDPOINTS.WORKFLOW.CREATE, workflow)
  return response.data
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
  await apiClient.put<void>(API_ENDPOINTS.WORKFLOW.UPDATE(id), workflow)
}

export const deleteWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('delete_workflow', { id })
    return
  }
  await apiClient.delete<void>(API_ENDPOINTS.WORKFLOW.DELETE(id))
}

export const executeInline = async (workflow: Workflow): Promise<ExecutionContext> => {
  if (isTauri()) {
    const { id } = await createWorkflow(workflow)
    return invokeCommand<ExecutionContext>('execute_workflow_sync', {
      workflow_id: id,
      input: {},
    })
  }
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
  if (isTauri()) {
    const taskId = await invokeCommand<string>('submit_workflow', {
      workflow_id: id,
      input: initialVariables || {},
    })
    return { execution_id: taskId }
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
    const items = await invokeCommand<ExecutionSummary[]>('list_workflow_executions', {
      workflow_id: id,
      limit: pageSize,
    })
    const total = items.length
    const resolvedPageSize = total > 0 ? Math.min(total, pageSize) : pageSize
    return {
      items,
      total,
      page: 1,
      page_size: resolvedPageSize,
      total_pages: total > 0 ? 1 : 0,
    }
  }
  const response = await apiClient.get<ExecutionHistoryPage>(API_ENDPOINTS.EXECUTION.HISTORY(id), {
    params: { page, page_size: pageSize },
  })
  return response.data
}
