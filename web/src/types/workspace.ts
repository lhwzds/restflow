// Workspace shared types

import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'
import type { ModelRef } from '@/types/generated/ModelRef'
import type { ModelId } from '@/types/generated/ModelId'
import type { Provider } from '@/types/generated/Provider'

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface RunListItem {
  id: string
  title: string
  status: string
  updatedAt: number
  runId?: string | null
  childRuns?: RunListItem[]
}

export interface WorkspaceSessionFolder {
  containerId: string
  sessionId: string
  name: string
  subtitle?: string | null
  status: SessionStatus
  updatedAt: number
  expanded: boolean
  agentId?: string
  agentName?: string
  sourceChannel?: ChatSessionSource | null
  runs: RunListItem[]
}

export interface BackgroundTaskFolder {
  taskId: string
  chatSessionId?: string | null
  name: string
  subtitle?: string | null
  status: string
  updatedAt: number
  expanded: boolean
  runs: RunListItem[]
}

export interface ExternalChannelFolder {
  containerId: string
  latestSessionId?: string | null
  name: string
  subtitle?: string | null
  status?: string | null
  updatedAt: number
  expanded: boolean
  sourceChannel?: ChatSessionSource | null
  runs: RunListItem[]
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
