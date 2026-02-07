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
import { API_ENDPOINTS } from '@/constants'

export interface ActiveTaskInfo {
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
}

/**
 * Event name for task stream events (matches Rust constant TASK_STREAM_EVENT)
 */
export const TASK_STREAM_EVENT = 'agent-task:stream'

/**
 * Request to create a new agent task
 */
export interface CreateAgentTaskRequest {
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
export interface UpdateAgentTaskRequest {
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

/**
 * List all agent tasks
 */
export async function listAgentTasks(): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('list_agent_tasks')
  }
  const response = await apiClient.get<AgentTask[]>(API_ENDPOINTS.AGENT_TASK.LIST)
  return response.data
}

/**
 * List agent tasks filtered by status
 */
export async function listAgentTasksByStatus(status: AgentTaskStatus): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('list_agent_tasks_by_status', { status })
  }
  const response = await apiClient.get<AgentTask[]>(API_ENDPOINTS.AGENT_TASK.LIST_BY_STATUS(status))
  return response.data
}

/**
 * Get an agent task by ID
 */
export async function getAgentTask(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('get_agent_task', { id })
  }
  const response = await apiClient.get<AgentTask>(API_ENDPOINTS.AGENT_TASK.GET(id))
  return response.data
}

/**
 * Create a new agent task
 */
export async function createAgentTask(request: CreateAgentTaskRequest): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('create_agent_task', { request })
  }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.AGENT_TASK.CREATE, request)
  return response.data
}

/**
 * Update an existing agent task
 */
export async function updateAgentTask(id: string, request: UpdateAgentTaskRequest): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('update_agent_task', { id, request })
  }
  const response = await apiClient.put<AgentTask>(API_ENDPOINTS.AGENT_TASK.UPDATE(id), request)
  return response.data
}

/**
 * Delete an agent task
 */
export async function deleteAgentTask(id: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('delete_agent_task', { id })
  }
  const response = await apiClient.delete<boolean>(API_ENDPOINTS.AGENT_TASK.DELETE(id))
  return response.data
}

/**
 * Pause an agent task
 */
export async function pauseAgentTask(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('pause_agent_task', { id })
  }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.AGENT_TASK.PAUSE(id))
  return response.data
}

/**
 * Resume a paused agent task
 */
export async function resumeAgentTask(id: string): Promise<AgentTask> {
  if (isTauri()) {
    return tauriInvoke<AgentTask>('resume_agent_task', { id })
  }
  const response = await apiClient.post<AgentTask>(API_ENDPOINTS.AGENT_TASK.RESUME(id))
  return response.data
}

/**
 * Get events for a specific task
 * @param taskId - The task ID
 * @param limit - Optional maximum number of events to return
 */
export async function getAgentTaskEvents(taskId: string, limit?: number): Promise<TaskEvent[]> {
  if (isTauri()) {
    return tauriInvoke<TaskEvent[]>('get_agent_task_events', { taskId, limit })
  }
  const url = limit
    ? `${API_ENDPOINTS.AGENT_TASK.EVENTS(taskId)}?limit=${limit}`
    : API_ENDPOINTS.AGENT_TASK.EVENTS(taskId)
  const response = await apiClient.get<TaskEvent[]>(url)
  return response.data
}

/**
 * Get tasks that are ready to run (based on their schedule)
 */
export async function getRunnableAgentTasks(): Promise<AgentTask[]> {
  if (isTauri()) {
    return tauriInvoke<AgentTask[]>('get_runnable_agent_tasks')
  }
  const response = await apiClient.get<AgentTask[]>(API_ENDPOINTS.AGENT_TASK.RUNNABLE)
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
export function formatTaskStatus(status: AgentTaskStatus): string {
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
export async function onTaskStreamEvent(
  callback: (event: TaskStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    // In web mode, return a no-op unlisten function
    console.warn('Task stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<TaskStreamEvent>(TASK_STREAM_EVENT, (event) => {
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
export async function onTaskStreamEventForTask(
  taskId: string,
  callback: (event: TaskStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('Task stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<TaskStreamEvent>(TASK_STREAM_EVENT, (event) => {
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
export async function runAgentTaskStreaming(taskId: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('Streaming task execution is only available in Tauri desktop app')
  }
  return tauriInvoke<void>('run_agent_task_streaming', { id: taskId })
}

/**
 * Get list of currently active (running) task IDs
 *
 * @returns Array of task IDs that are currently running
 */
export async function getActiveAgentTasks(): Promise<ActiveTaskInfo[]> {
  if (!isTauri()) {
    return []
  }
  return tauriInvoke<ActiveTaskInfo[]>('get_active_agent_tasks')
}

/**
 * Cancel a running task
 *
 * @param taskId - ID of the task to cancel
 * @returns true if cancellation was requested successfully
 */
export async function cancelAgentTask(taskId: string): Promise<boolean> {
  if (!isTauri()) {
    throw new Error('Task cancellation is only available in Tauri desktop app')
  }
  return tauriInvoke<boolean>('cancel_agent_task', { taskId })
}
