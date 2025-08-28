import { apiClient } from './config'
import type { ActiveTrigger } from '@/types/generated/ActiveTrigger'
import type { TriggerStatus } from '@/types/generated/TriggerStatus'

export const activateWorkflow = async (id: string): Promise<void> => {
  await apiClient.put<{
    status: string
    message: string
  }>(`/api/workflow/${id}/activate`)
}

export const deactivateWorkflow = async (id: string): Promise<void> => {
  await apiClient.put<{
    status: string
    message: string
  }>(`/api/workflow/${id}/deactivate`)
}

export const getTriggerStatus = async (id: string): Promise<TriggerStatus | null> => {
  const response = await apiClient.get<{
    status: string
    data: TriggerStatus
  }>(`/api/workflow/${id}/trigger-status`)
  
  return response.data?.data || null
}

export const testWorkflow = async (id: string, testData?: any): Promise<any> => {
  const response = await apiClient.post<{
    status: string
    data: any
  }>(`/api/workflow/${id}/test`, testData)
  return response.data.data
}

export const listActiveTriggers = async (): Promise<ActiveTrigger[]> => {
  const response = await apiClient.get<{
    status: string
    data: ActiveTrigger[]
  }>('/api/triggers/list-active')
  return response.data.data
}

export const getWebhookUrl = (id: string): string => {
  return `${apiClient.defaults.baseURL}/api/triggers/webhook/${id}`
}