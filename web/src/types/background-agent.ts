import type { BackgroundAgent as GeneratedBackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { BackgroundAgentStatus as GeneratedBackgroundAgentStatus } from '@/types/generated/BackgroundAgentStatus'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { TaskSchedule } from '@/types/generated/TaskSchedule'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'

export type BackgroundAgent = GeneratedBackgroundAgent
export type BackgroundAgentStatus = GeneratedBackgroundAgentStatus
export type BackgroundAgentEvent = TaskEvent
export type BackgroundAgentSchedule = TaskSchedule
export type BackgroundAgentStreamEvent = TaskStreamEvent
