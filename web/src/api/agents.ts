import { invokeCommand } from './tauri-client'
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
  return invokeCommand('listAgents')
}

export async function getAgent(id: string): Promise<StoredAgent> {
  return invokeCommand('getAgent', id)
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  return invokeCommand('createAgent', data)
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  return invokeCommand('updateAgent', id, {
    name: data.name ?? null,
    agent: data.agent ?? null,
  })
}

export async function deleteAgent(id: string): Promise<void> {
  await invokeCommand('deleteAgent', id)
}
