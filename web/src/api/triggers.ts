import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
import type { TriggerStatus } from '@/types/generated/TriggerStatus'
import type { TestWorkflowResponse } from '@/types/api'
import { API_ENDPOINTS } from '@/constants'

export const activateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('activate_workflow', { workflow_id: id })
    return
  }
  await apiClient.put(API_ENDPOINTS.TRIGGER.ACTIVATE(id))
}

export const deactivateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('deactivate_workflow', { workflow_id: id })
    return
  }
  await apiClient.put(API_ENDPOINTS.TRIGGER.DEACTIVATE(id))
}

export const getTriggerStatus = async (id: string): Promise<TriggerStatus | null> => {
  if (isTauri()) {
    return invokeCommand<TriggerStatus | null>('get_trigger_status', { workflow_id: id })
  }
  const response = await apiClient.get<TriggerStatus>(API_ENDPOINTS.TRIGGER.STATUS(id))
  return response.data || null
}

export const testWorkflow = async (id: string, testData?: any): Promise<TestWorkflowResponse> => {
  if (isTauri()) {
    return invokeCommand<TestWorkflowResponse>('test_workflow', {
      workflow_id: id,
      test_data: testData || {},
    })
  }
  const payload = testData ?? {}
  const response = await apiClient.post<TestWorkflowResponse>(
    API_ENDPOINTS.TRIGGER.TEST(id),
    payload,
    {
      headers: {
        'Content-Type': 'application/json',
      },
    },
  )
  return response.data
}

export const getWebhookUrl = (id: string): string => {
  return `${apiClient.defaults.baseURL}${API_ENDPOINTS.TRIGGER.WEBHOOK(id)}`
}
