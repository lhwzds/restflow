import { apiClient } from './config'
import { isTauri, invokeCommand } from './utils'
import type { TriggerStatus } from '@/types/generated/TriggerStatus'
import { API_ENDPOINTS } from '@/constants'

export const activateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('activate_workflow', { workflow_id: id })
    return
  }
  await apiClient.put<{
    status: string
    message: string
  }>(API_ENDPOINTS.TRIGGER.ACTIVATE(id))
}

export const deactivateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    await invokeCommand('deactivate_workflow', { workflow_id: id })
    return
  }
  await apiClient.put<{
    status: string
    message: string
  }>(API_ENDPOINTS.TRIGGER.DEACTIVATE(id))
}

export const getTriggerStatus = async (id: string): Promise<TriggerStatus | null> => {
  if (isTauri()) {
    return invokeCommand<TriggerStatus | null>('get_trigger_status', { workflow_id: id })
  }
  const response = await apiClient.get<{
    status: string
    data: TriggerStatus
  }>(API_ENDPOINTS.TRIGGER.STATUS(id))
  
  return response.data?.data || null
}

export const testWorkflow = async (id: string, testData?: any): Promise<any> => {
  if (isTauri()) {
    return invokeCommand('test_workflow', { 
      workflow_id: id, 
      test_data: testData || {} 
    })
  }
  const response = await apiClient.post<{
    status: string
    data: any
  }>(API_ENDPOINTS.TRIGGER.TEST(id), testData)
  return response.data.data
}

export const getWebhookUrl = (id: string): string => {
  return `${apiClient.defaults.baseURL}${API_ENDPOINTS.TRIGGER.WEBHOOK(id)}`
}