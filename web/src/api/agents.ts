import { requestTyped } from './http-client'
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
  return requestTyped<StoredAgent[]>({ type: 'ListAgents' })
}

export async function getAgent(id: string): Promise<StoredAgent> {
  return requestTyped<StoredAgent>({ type: 'GetAgent', data: { id } })
}

export async function createAgent(data: CreateAgentRequest): Promise<StoredAgent> {
  return requestTyped<StoredAgent>({
    type: 'CreateAgent',
    data,
  })
}

export async function updateAgent(id: string, data: UpdateAgentRequest): Promise<StoredAgent> {
  return requestTyped<StoredAgent>({
    type: 'UpdateAgent',
    data: {
      id,
      name: data.name ?? null,
      agent: data.agent ?? null,
    },
  })
}

export async function deleteAgent(id: string): Promise<void> {
  await requestTyped({ type: 'DeleteAgent', data: { id } })
}
