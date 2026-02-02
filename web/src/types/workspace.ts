// Workspace shared types

export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface Task {
  id: string
  name: string
  status: TaskStatus
  createdAt: number
}

export type SessionStatus = TaskStatus

export interface SessionItem {
  id: string
  name: string
  status: SessionStatus
  updatedAt: number
  agentId?: string
  agentName?: string
}

export type StepType = 'skill_read' | 'script_run' | 'api_call' | 'thinking'
export type StepStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface ExecutionStep {
  type: StepType
  name: string
  status: StepStatus
  duration?: number
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

export interface FileItem<T = unknown> {
  id: string
  name: string
  path: string
  isDirectory: boolean
  childCount?: number
  updatedAt?: number
  // Store the original data (Skill or StoredAgent) for preview/edit
  data?: T
}

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}
