// Workspace shared types

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface SessionItem {
  id: string
  name: string
  status: SessionStatus
  updatedAt: number
  agentId?: string
  agentName?: string
}

export interface AgentFile {
  id: string
  name: string
  path: string
}

export interface ModelOption {
  id: string
  name: string
}
