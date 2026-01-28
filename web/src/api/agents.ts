import { apiClient, isTauri, tauriInvoke } from './config'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import type { AgentExecuteResponse } from '@/types/generated/AgentExecuteResponse'
import type { ExecutionDetails } from '@/types/generated/ExecutionDetails'
import type { ExecutionStep } from '@/types/generated/ExecutionStep'
import type { ToolCallInfo } from '@/types/generated/ToolCallInfo'
import { API_ENDPOINTS } from '@/constants'

// Re-export generated types for convenience
export type { AgentExecuteResponse, ExecutionDetails, ExecutionStep, ToolCallInfo }

export interface CreateAgentRequest {
  name: string
  agent: AgentNode
}

export interface UpdateAgentRequest {
  name?: string
  agent?: AgentNode
}

export async function listAgents(): Promise<StoredAgent[]> {
  if (isTauri()) {
    return tauriInvoke<StoredAgent[]>('list_agents')
  }
  const response = await apiClient.get<StoredAgent[]>(API_ENDPOINTS.AGENT.LIST)
  return response.data
}

export async function getAgent(id: string): Promise<StoredAgent> {
  if (isTauri()) {
    return tauriInvoke<StoredAgent>('get_agent', { id })
  }
  const response = await apiClient.get<StoredAgent>(API_ENDPOINTS.AGENT.GET(id))
  return response.data
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  if (isTauri()) {
    return tauriInvoke<StoredAgent>('create_agent', { request: data })
  }
  const response = await apiClient.post<StoredAgent>(API_ENDPOINTS.AGENT.CREATE, data)
  return response.data
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  if (isTauri()) {
    return tauriInvoke<StoredAgent>('update_agent', { id, request: data })
  }
  const response = await apiClient.put<StoredAgent>(API_ENDPOINTS.AGENT.UPDATE(id), data)
  return response.data
}

export async function deleteAgent(id: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('delete_agent', { id })
  }
  await apiClient.delete(API_ENDPOINTS.AGENT.DELETE(id))
}

export async function executeAgent(id: string, input: string): Promise<AgentExecuteResponse> {
  if (isTauri()) {
    // Note: Agent execution in Tauri is currently a placeholder
    return tauriInvoke<AgentExecuteResponse>('execute_agent', {
      id,
      request: { prompt: input },
    })
  }
  const response = await apiClient.post<AgentExecuteResponse>(API_ENDPOINTS.AGENT.EXECUTE(id), {
    input,
  })
  return response.data
}

export async function executeAgentInline(
  agent: AgentNode,
  input: string,
): Promise<AgentExecuteResponse> {
  if (isTauri()) {
    // Note: Inline agent execution in Tauri is currently a placeholder
    return tauriInvoke<AgentExecuteResponse>('execute_agent_inline', {
      agent,
      prompt: input,
    })
  }
  const response = await apiClient.post<AgentExecuteResponse>(API_ENDPOINTS.AGENT.EXECUTE_INLINE, {
    agent,
    input,
  })
  return response.data
}
