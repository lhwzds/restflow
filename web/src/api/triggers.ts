import { apiClient, isTauri, API_BASE_URL } from './config'
import type { TriggerStatus } from '@/types/generated/TriggerStatus'
import type { TestWorkflowResponse } from '@/types/api'
import { API_ENDPOINTS } from '@/constants'

// Helper to throw Tauri not supported error
function throwTauriNotSupported(operation: string): never {
  throw new Error(
    `${operation} is not yet supported in Tauri mode. Triggers require server mode for webhooks and scheduled execution.`,
  )
}

export const activateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    throwTauriNotSupported('Workflow activation')
  }
  await apiClient.put(API_ENDPOINTS.TRIGGER.ACTIVATE(id))
}

export const deactivateWorkflow = async (id: string): Promise<void> => {
  if (isTauri()) {
    throwTauriNotSupported('Workflow deactivation')
  }
  await apiClient.put(API_ENDPOINTS.TRIGGER.DEACTIVATE(id))
}

export const getTriggerStatus = async (id: string): Promise<TriggerStatus | null> => {
  if (isTauri()) {
    // Return null to indicate no trigger status in Tauri mode
    return null
  }
  const response = await apiClient.get<TriggerStatus>(API_ENDPOINTS.TRIGGER.STATUS(id))
  return response.data || null
}

export const testWorkflow = async (
  id: string,
  testData?: unknown,
): Promise<TestWorkflowResponse> => {
  if (isTauri()) {
    throwTauriNotSupported('Workflow test')
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
  if (isTauri()) {
    // In Tauri mode, webhooks are not available
    return ''
  }
  return `${API_BASE_URL}${API_ENDPOINTS.TRIGGER.WEBHOOK(id)}`
}
