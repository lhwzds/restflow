// Workspace shared types

import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'
import type { ModelRef } from '@/types/generated/ModelRef'
import type { AIModel } from '@/types/generated/AIModel'

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface SessionItem {
  id: string
  name: string
  status: SessionStatus
  updatedAt: number
  agentId?: string
  agentName?: string
  sourceChannel?: ChatSessionSource | null
  /** Indicates the chat session is bound to a background agent task. */
  isBackgroundSession?: boolean
  /** If true, this item represents a background agent, not a chat session */
  isBackgroundAgent?: boolean
}

export interface AgentFile {
  id: string
  name: string
  path: string
}

export interface WorkspaceAgentModelSelection {
  id: string
  name: string
  model: AIModel
  model_ref: ModelRef
}

export interface ModelOption {
  id: string
  name: string
}
