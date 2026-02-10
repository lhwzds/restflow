import { tauriInvoke } from './tauri-client'
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
  return tauriInvoke<StoredAgent[]>('list_agents')
}

export async function getAgent(id: string): Promise<StoredAgent> {
  return tauriInvoke<StoredAgent>('get_agent', { id })
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  return tauriInvoke<StoredAgent>('create_agent', { request: data })
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  return tauriInvoke<StoredAgent>('update_agent', { id, request: data })
}

export async function deleteAgent(id: string): Promise<void> {
  return tauriInvoke<void>('delete_agent', { id })
}
