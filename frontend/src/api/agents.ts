import { apiClient } from './config'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import { API_ENDPOINTS } from '@/constants'

export interface CreateAgentRequest {
  name: string
  agent: AgentNode
}

export interface UpdateAgentRequest {
  name?: string
  agent?: AgentNode
}

export async function listAgents(): Promise<StoredAgent[]> {
  const response = await apiClient.get(API_ENDPOINTS.AGENT.LIST)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to fetch agents')
}

export async function getAgent(id: string): Promise<StoredAgent> {
  const response = await apiClient.get(API_ENDPOINTS.AGENT.GET(id))
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to fetch agent')
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.post(API_ENDPOINTS.AGENT.CREATE, data)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to create agent')
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.put(API_ENDPOINTS.AGENT.UPDATE(id), data)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to update agent')
}

export async function deleteAgent(id: string): Promise<void> {
  const response = await apiClient.delete(API_ENDPOINTS.AGENT.DELETE(id))
  if (response.data.status !== 'success') {
    throw new Error(response.data.message || 'Failed to delete agent')
  }
}

export async function executeAgent(id: string, input: string): Promise<string> {
  const response = await apiClient.post(API_ENDPOINTS.AGENT.EXECUTE(id), { input })
  if (response.data.status === 'success') {
    return response.data.data.response
  }
  throw new Error(response.data.message || 'Failed to execute agent')
}

export async function executeAgentInline(agent: any, input: string): Promise<string> {
  const response = await apiClient.post(API_ENDPOINTS.AGENT.EXECUTE_INLINE, { agent, input })
  if (response.data.status === 'success') {
    return response.data.data.response
  }
  throw new Error(response.data.message || 'Failed to execute agent')
}