// Workspace shared types

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface SessionItem {
  id: string
  name: string
  status: SessionStatus
  updatedAt: number
  agentId?: string
  agentName?: string
  /** If true, this item represents a background agent, not a chat session */
  isBackgroundAgent?: boolean
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
