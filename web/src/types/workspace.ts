// Workspace shared types

import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'
import type { ModelRef } from '@/types/generated/ModelRef'
import type { ModelId } from '@/types/generated/ModelId'
import type { Provider } from '@/types/generated/Provider'

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface SessionItem {
  id: string
  name: string
  status: SessionStatus
  updatedAt: number
  subtitle?: string | null
  agentId?: string
  agentName?: string
  containerId?: string | null
  sourceChannel?: ChatSessionSource | null
  /** Indicates the chat session is bound to a background agent task. */
  isBackgroundSession?: boolean
  /** Background task ID when the session is bound to a background agent. */
  backgroundTaskId?: string | null
  /** If true, this item represents a background agent, not a chat session */
  isBackgroundAgent?: boolean
}

export interface BackgroundRunItem {
  id: string
  title: string
  status: string
  updatedAt: number
  runId?: string | null
}

export interface BackgroundTaskFolder {
  taskId: string
  name: string
  subtitle?: string | null
  status: string
  updatedAt: number
  expanded: boolean
  runs: BackgroundRunItem[]
}

export interface ExternalChannelFolder {
  containerId: string
  name: string
  subtitle?: string | null
  status?: string | null
  updatedAt: number
  expanded: boolean
  sourceChannel?: ChatSessionSource | null
  sessions: SessionItem[]
}

export interface AgentFile {
  id: string
  name: string
  path: string
}

export interface WorkspaceAgentModelSelection {
  id: string
  name: string
  model: ModelId
  model_ref: ModelRef
}

export interface ModelOption {
  id: string
  name: string
  provider: Provider
}
