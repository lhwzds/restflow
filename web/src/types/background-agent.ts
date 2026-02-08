import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { TaskSchedule } from '@/types/generated/TaskSchedule'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'

export type BackgroundAgent = AgentTask
export type BackgroundAgentStatus = AgentTaskStatus
export type BackgroundAgentEvent = TaskEvent
export type BackgroundAgentSchedule = TaskSchedule
export type BackgroundAgentStreamEvent = TaskStreamEvent
