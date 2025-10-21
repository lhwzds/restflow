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
  const response = await apiClient.get<StoredAgent[]>(API_ENDPOINTS.AGENT.LIST)
  return response.data
}

export async function getAgent(id: string): Promise<StoredAgent> {
  const response = await apiClient.get<StoredAgent>(API_ENDPOINTS.AGENT.GET(id))
  return response.data
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.post<StoredAgent>(API_ENDPOINTS.AGENT.CREATE, data)
  return response.data
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  const response = await apiClient.put<StoredAgent>(API_ENDPOINTS.AGENT.UPDATE(id), data)
  return response.data
}

export async function deleteAgent(id: string): Promise<void> {
  await apiClient.delete(API_ENDPOINTS.AGENT.DELETE(id))
}

export async function executeAgent(id: string, input: string): Promise<string> {
  const response = await apiClient.post<{ response: string }>(
    API_ENDPOINTS.AGENT.EXECUTE(id),
    { input }
  )
  return response.data.response
}

export async function executeAgentInline(agent: any, input: string): Promise<string> {
  const response = await apiClient.post<{ response: string }>(
    API_ENDPOINTS.AGENT.EXECUTE_INLINE,
    { agent, input }
  )
  return response.data.response
}

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export async function getChatHistory(id: string): Promise<ChatMessage[]> {
  const response = await apiClient.get<ChatMessage[]>(`/api/agents/${id}/chat-history`)
  return response.data
}