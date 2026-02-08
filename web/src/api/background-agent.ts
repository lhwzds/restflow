/**
 * Agent Task API
 *
 * Provides API functions for managing scheduled agent tasks, including
 * CRUD operations, status management, and event retrieval.
 */

import { apiClient, isTauri, tauriInvoke } from './config'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { TaskSchedule } from '@/types/generated/TaskSchedule'
import type { NotificationConfig } from '@/types/generated/NotificationConfig'
import type { ExecutionMode } from '@/types/generated/ExecutionMode'
import type { MemoryConfig } from '@/types/generated/MemoryConfig'
import type { MemoryScope } from '@/types/generated/MemoryScope'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { BackgroundAgentControlAction } from '@/types/generated/BackgroundAgentControlAction'
import type { BackgroundProgress } from '@/types/generated/BackgroundProgress'
import type { BackgroundMessage } from '@/types/generated/BackgroundMessage'
import type { BackgroundMessageSource } from '@/types/generated/BackgroundMessageSource'
import { API_ENDPOINTS } from '@/constants'

export interface ActiveBackgroundAgentInfo {
  task_id: string
  task_name: string
  agent_id: string
  started_at: number
  execution_mode: string
}

// Re-export types for convenience
export type {
  AgentTask,
  AgentTaskStatus,
  TaskEvent,
  TaskSchedule,
  NotificationConfig,
  ExecutionMode,
  MemoryConfig,
  MemoryScope,
  TaskStreamEvent,
  StreamEventKind,
  BackgroundAgentControlAction,
  BackgroundProgress,
  BackgroundMessage,
  BackgroundMessageSource,
}

/**
 * Event name for task stream events (matches Rust constant BACKGROUND_AGENT_STREAM_EVENT)
 */
export const BACKGROUND_AGENT_STREAM_EVENT = 'background-agent:stream'

/**
 * Request to create a new agent task
 */
export interface CreateBackgroundAgentRequest {
  /** Display name of the task */
  name: string
  /** ID of the agent to execute */
  agent_id: string
  /** Schedule configuration */
  schedule: TaskSchedule
  /** Optional description */
  description?: string
  /** Optional input/prompt to send to the agent */
  input?: string
  /** Optional runtime template used to render task input */
  input_template?: string
  /** Optional notification configuration */
  notification?: NotificationConfig
  /** Optional execution mode (API or CLI) */
  execution_mode?: ExecutionMode
  /** Optional memory configuration */
  memory?: MemoryConfig
  /** Optional memory scope override */
  memory_scope?: MemoryScope
}

/**
 * Request to update an existing agent task
 */
export interface UpdateBackgroundAgentRequest {
  /** New display name (optional) */
  name?: string
  /** New description (optional) */
  description?: string
  /** New agent ID (optional) */
  agent_id?: string
  /** New input/prompt (optional) */
  input?: string
  /** New runtime template (optional) */
  input_template?: string
  /** New schedule (optional) */
  schedule?: TaskSchedule
  /** New notification config (optional) */
  notification?: NotificationConfig
  /** New memory config (optional) */
  memory?: MemoryConfig
  /** New memory scope override (optional) */
  memory_scope?: MemoryScope
}

interface ControlBackgroundAgentRequest {
  action: BackgroundAgentControlAction
}

interface SendBackgroundMessageRequest {
  message: string
  source?: BackgroundMessageSource
}

/**
 * List all agent tasks
 */
export async function listBackgroundAgents(): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('list_background_agents')
  }
  const response = await apiClient.get<AgentTask[]>(API_ENDPOINTS.BACKGROUND_AGENT.LIST)
  return response.data
}

/**
 * List agent tasks filtered by status
 */
export async function listBackgroundAgentsByStatus(status: AgentTaskStatus): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('list_background_agents_by_status', { status })
  }
  const response = await apiClient.get<AgentTask[]>(
    API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS(status),
  )
  return response.data
}

/**
 * Get an agent task by ID
 */
export async function getBackgroundAgent(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('get_background_agent', { id })
  }
  const response = await apiClient.get<AgentTask>(API_ENDPOINTS.BACKGROUND_AGENT.GET(id))
  return response.data
}

/**
 * Create a new agent task
 */
export async function createBackgroundAgent(request: CreateBackgroundAgentRequest): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('create_background_agent', { request })
  }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.BACKGROUND_AGENT.CREATE, request)
  return response.data
}

/**
 * Update an existing agent task
 */
export async function updateBackgroundAgent(
  id: string,
  request: UpdateBackgroundAgentRequest,
): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('update_background_agent', { id, request })
  }
  const response = await apiClient.patch<AgentTask>(
    API_ENDPOINTS.BACKGROUND_AGENT.UPDATE(id),
    request,
  )
  return response.data
}

/**
 * Delete an agent task
 */
export async function deleteBackgroundAgent(id: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('delete_background_agent', { id })
  }
  const response = await apiClient.delete<boolean>(API_ENDPOINTS.BACKGROUND_AGENT.DELETE(id))
  return response.data
}

/**
 * Pause an agent task
 */
export async function pauseBackgroundAgent(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('pause_background_agent', { id })
  }
  const body: ControlBackgroundAgentRequest = { action: 'pause' }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.BACKGROUND_AGENT.CONTROL(id), body)
  return response.data
}

/**
 * Resume a paused agent task
 */
export async function resumeBackgroundAgent(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('resume_background_agent', { id })
  }
  const body: ControlBackgroundAgentRequest = { action: 'resume' }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.BACKGROUND_AGENT.CONTROL(id), body)
  return response.data
}

/**
 * Get events for a specific task
 * @param taskId - The task ID
 * @param limit - Optional maximum number of events to return
 */
export async function getBackgroundAgentEvents(taskId: string, limit?: number): Promise<TaskEvent[]> {
  if (isTauri()) {
    return tauriInvoke<TaskEvent[]>('get_background_agent_events', {
      taskId,
      limit,
    })
  }
  const url = limit
    ? `${API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS(taskId)}?event_limit=${limit}`
    : API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS(taskId)
  const response = await apiClient.get<BackgroundProgress>(url)
  return response.data.recent_events
}

/**
 * Get tasks that are ready to run (based on their schedule)
 */
export async function getRunnableBackgroundAgents(): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('get_runnable_background_agents')
  }
  const response = await apiClient.get<AgentTask[]>(
    API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS('active'),
  )
  const now = Date.now()
  return response.data.filter(
    (task) => task.status === 'active' && (task.next_run_at === null || task.next_run_at <= now),
  )
}

/**
 * Send an interaction message to a background agent.
 */
export async function sendBackgroundAgentMessage(
  id: string,
  request: SendBackgroundMessageRequest,
): Promise<BackgroundMessage> {
  const response = await apiClient.post<BackgroundMessage>(
    API_ENDPOINTS.BACKGROUND_AGENT.MESSAGES(id),
    request,
  )
  return response.data
}

/**
 * List recent interaction messages for a background agent.
 */
export async function listBackgroundAgentMessages(
  id: string,
  limit = 50,
): Promise<BackgroundMessage[]> {
  const response = await apiClient.get<BackgroundMessage[]>(
    `${API_ENDPOINTS.BACKGROUND_AGENT.MESSAGES(id)}?limit=${limit}`,
  )
  return response.data
}

/**
 * Helper to create a default notification config
 */
export function createDefaultNotificationConfig(): NotificationConfig {
  return {
    telegram_enabled: false,
    telegram_bot_token: null,
    telegram_chat_id: null,
    notify_on_failure_only: false,
    include_output: true,
  }
}

/**
 * Helper to create a one-time schedule
 * @param runAt - Unix timestamp in milliseconds
 */
export function createOnceSchedule(runAt: number): TaskSchedule {
  return {
    type: 'once',
    run_at: runAt,
  }
}

/**
 * Helper to create an interval schedule
 * @param intervalMs - Interval in milliseconds
 * @param startAt - Optional start time (defaults to now)
 */
export function createIntervalSchedule(intervalMs: number, startAt?: number): TaskSchedule {
  return {
    type: 'interval',
    interval_ms: intervalMs,
    start_at: startAt ?? null,
  }
}

/**
 * Helper to create a cron schedule
 * @param expression - Cron expression (e.g., "0 9 * * *")
 * @param timezone - Optional timezone (e.g., "America/Los_Angeles")
 */
export function createCronSchedule(expression: string, timezone?: string): TaskSchedule {
  return {
    type: 'cron',
    expression,
    timezone: timezone ?? null,
  }
}

/**
 * Format schedule for display
 */
export function formatSchedule(schedule: TaskSchedule): string {
  switch (schedule.type) {
    case 'once':
      return `Once at ${new Date(schedule.run_at).toLocaleString()}`
    case 'interval': {
      const hours = Math.floor(schedule.interval_ms / 3600000)
      const minutes = Math.floor((schedule.interval_ms % 3600000) / 60000)
      if (hours > 0 && minutes > 0) {
        return `Every ${hours}h ${minutes}m`
      } else if (hours > 0) {
        return `Every ${hours} hour${hours > 1 ? 's' : ''}`
      } else {
        return `Every ${minutes} minute${minutes > 1 ? 's' : ''}`
      }
    }
    case 'cron':
      return `Cron: ${schedule.expression}${schedule.timezone ? ` (${schedule.timezone})` : ''}`
    default:
      return 'Unknown schedule'
  }
}

/**
 * Format task status for display
 */
export function formatBackgroundAgentStatus(status: AgentTaskStatus): string {
  const statusMap: Record<AgentTaskStatus, string> = {
    active: 'Active',
    paused: 'Paused',
    running: 'Running',
    completed: 'Completed',
    failed: 'Failed',
  }
  return statusMap[status] || status
}

/**
 * Get status badge color class
 */
export function getStatusColor(status: AgentTaskStatus): string {
  const colorMap: Record<AgentTaskStatus, string> = {
    active: 'success',
    paused: 'info',
    running: 'primary',
    completed: 'success',
    failed: 'danger',
  }
  return colorMap[status] || 'default'
}

// ============================================================================
// Task Stream Event Handling
// ============================================================================

/**
 * Listen for all task stream events
 *
 * This registers a listener for all task execution events. Use this for
 * global monitoring or when managing multiple concurrent tasks.
 *
 * @param callback - Function to call with each event
 * @returns Unlisten function to stop listening
 */
export async function onBackgroundAgentStreamEvent(
  callback: (event: TaskStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    // In web mode, return a no-op unlisten function
    console.warn('Task stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<TaskStreamEvent>(BACKGROUND_AGENT_STREAM_EVENT, (event) => {
    callback(event.payload)
  })
}

/**
 * Listen for stream events for a specific task
 *
 * This filters events to only those belonging to the specified task.
 * Use this when monitoring a single task's execution.
 *
 * @param taskId - Task ID to filter events
 * @param callback - Function to call with each matching event
 * @returns Unlisten function to stop listening
 */
export async function onBackgroundAgentStreamEventForAgent(
  taskId: string,
  callback: (event: TaskStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('Task stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<TaskStreamEvent>(BACKGROUND_AGENT_STREAM_EVENT, (event) => {
    if (event.payload.task_id === taskId) {
      callback(event.payload)
    }
  })
}

/**
 * Type guard for checking event kind
 */
export function isEventKind<T extends StreamEventKind['type']>(
  event: TaskStreamEvent,
  type: T,
): event is TaskStreamEvent & { kind: Extract<StreamEventKind, { type: T }> } {
  return event.kind.type === type
}

/**
 * Run a task with streaming support
 *
 * This is a convenience wrapper that invokes the Tauri command to run
 * a task with real-time event streaming.
 *
 * @param taskId - ID of the task to run
 * @returns Promise that resolves when the task starts (not when it completes)
 */
export async function runBackgroundAgentStreaming(taskId: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('Streaming task execution is only available in Tauri desktop app')
  }
  return tauriInvoke<void>('run_background_agent_streaming', { id: taskId })
}

/**
 * Get list of currently active (running) task IDs
 *
 * @returns Array of task IDs that are currently running
 */
export async function getActiveBackgroundAgents(): Promise<ActiveBackgroundAgentInfo[]> {
  if (!isTauri()) {
    return []
  }
  return tauriInvoke<ActiveBackgroundAgentInfo[]>('get_active_background_agents')
}

/**
 * Cancel a running task
 *
 * @param taskId - ID of the task to cancel
 * @returns true if cancellation was requested successfully
 */
export async function cancelBackgroundAgent(taskId: string): Promise<boolean> {
  if (!isTauri()) {
    throw new Error('Task cancellation is only available in Tauri desktop app')
  }
  return tauriInvoke<boolean>('cancel_background_agent', { taskId })
}
