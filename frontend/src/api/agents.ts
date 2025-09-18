import { apiClient } from './config'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'

export interface CreateAgentRequest {
  name: string
  agent: AgentNode
}

export interface UpdateAgentRequest {
  name?: string
  agent?: AgentNode
}

export async function listAgents(): Promise<StoredAgent[]> {
  const response = await apiClient.get('/api/agents')
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to fetch agents')
}

export async function getAgent(id: string): Promise<StoredAgent> {
  const response = await apiClient.get(`/api/agents/${id}`)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to fetch agent')
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.post('/api/agents', data)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to create agent')
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.put(`/api/agents/${id}`, data)
  if (response.data.status === 'success') {
    return response.data.data
  }
  throw new Error(response.data.message || 'Failed to update agent')
}

export async function deleteAgent(id: string): Promise<void> {
  const response = await apiClient.delete(`/api/agents/${id}`)
  if (response.data.status !== 'success') {
    throw new Error(response.data.message || 'Failed to delete agent')
  }
}

export async function executeAgent(id: string, input: string): Promise<string> {
  const response = await apiClient.post(`/api/agents/${id}/execute`, { input })
  if (response.data.status === 'success') {
    return response.data.data.response
  }
  throw new Error(response.data.message || 'Failed to execute agent')
}

export async function executeAgentInline(agent: any, input: string): Promise<string> {
  const response = await apiClient.post('/api/agents/execute-inline', { agent, input })
  if (response.data.status === 'success') {
    return response.data.data.response
  }
  throw new Error(response.data.message || 'Failed to execute agent')
}