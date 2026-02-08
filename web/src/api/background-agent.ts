/**
 * Background Agent API
 *
 * Provides API functions for managing scheduled background agents, including
 * CRUD operations, status management, and event retrieval.
 */

import { apiClient, isTauri, tauriInvoke } from './config'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  BackgroundAgent,
  BackgroundAgentEvent,
  BackgroundAgentSchedule,
  BackgroundAgentStatus,
  BackgroundAgentStreamEvent,
} from '@/types/background-agent'
import type { NotificationConfig } from '@/types/generated/NotificationConfig'
import type { ExecutionMode } from '@/types/generated/ExecutionMode'
import type { MemoryConfig } from '@/types/generated/MemoryConfig'
import type { MemoryScope } from '@/types/generated/MemoryScope'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { BackgroundAgentControlAction } from '@/types/generated/BackgroundAgentControlAction'
import type { BackgroundProgress } from '@/types/generated/BackgroundProgress'
import type { BackgroundMessage } from '@/types/generated/BackgroundMessage'
import type { BackgroundMessageSource } from '@/types/generated/BackgroundMessageSource'
import { API_ENDPOINTS } from '@/constants'

interface ActiveBackgroundAgentInfoPayload {
  task_id: string
  task_name: string
  agent_id: string
  started_at: number
  execution_mode: string
}

export interface ActiveBackgroundAgentInfo {
  background_agent_id: string
  background_agent_name: string
  executor_agent_id: string
  started_at: number
  execution_mode: string
}

// Re-export types for convenience
export type {
  BackgroundAgent,
  BackgroundAgentStatus,
  BackgroundAgentEvent,
  BackgroundAgentSchedule,
  NotificationConfig,
  ExecutionMode,
  MemoryConfig,
  MemoryScope,
  BackgroundAgentStreamEvent,
  StreamEventKind,
  BackgroundAgentControlAction,
  BackgroundProgress,
  BackgroundMessage,
  BackgroundMessageSource,
}

/**
 * Event name for background agent stream events (matches Rust constant BACKGROUND_AGENT_STREAM_EVENT)
 */
export const BACKGROUND_AGENT_STREAM_EVENT = 'background-agent:stream'

/**
 * Request to create a new background agent
 */
export interface CreateBackgroundAgentRequest {
  /** Display name of the background agent */
  name: string
  /** ID of the executor agent */
  agent_id: string
  /** Schedule configuration */
  schedule: BackgroundAgentSchedule
  /** Optional description */
  description?: string
  /** Optional input/prompt to send to the executor agent */
  input?: string
  /** Optional runtime template used to render background agent input */
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
 * Request to update an existing background agent
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
  schedule?: BackgroundAgentSchedule
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
 * List all background agents.
 */
export async function listBackgroundAgents(): Promise<BackgroundAgent[]> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent[]>('list_background_agents')
  }
  const response = await apiClient.get<BackgroundAgent[]>(API_ENDPOINTS.BACKGROUND_AGENT.LIST)
  return response.data
}

/**
 * List background agents filtered by status.
 */
export async function listBackgroundAgentsByStatus(
  status: BackgroundAgentStatus,
): Promise<BackgroundAgent[]> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent[]>('list_background_agents_by_status', { status })
  }
  const response = await apiClient.get<BackgroundAgent[]>(
    API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS(status),
  )
  return response.data
}

/**
 * Get a background agent by ID.
 */
export async function getBackgroundAgent(id: string): Promise<BackgroundAgent> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent>('get_background_agent', { id })
  }
  const response = await apiClient.get<BackgroundAgent>(API_ENDPOINTS.BACKGROUND_AGENT.GET(id))
  return response.data
}

/**
 * Create a new background agent.
 */
export async function createBackgroundAgent(
  request: CreateBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent>('create_background_agent', { request })
  }
  const response = await apiClient.post<BackgroundAgent>(
    API_ENDPOINTS.BACKGROUND_AGENT.CREATE,
    request,
  )
  return response.data
}

/**
 * Update an existing background agent.
 */
export async function updateBackgroundAgent(
  id: string,
  request: UpdateBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent>('update_background_agent', { id, request })
  }
  const response = await apiClient.patch<BackgroundAgent>(
    API_ENDPOINTS.BACKGROUND_AGENT.UPDATE(id),
    request,
  )
  return response.data
}

/**
 * Delete a background agent.
 */
export async function deleteBackgroundAgent(id: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('delete_background_agent', { id })
  }
  const response = await apiClient.delete<boolean>(API_ENDPOINTS.BACKGROUND_AGENT.DELETE(id))
  return response.data
}

/**
 * Pause a background agent.
 */
export async function pauseBackgroundAgent(id: string): Promise<BackgroundAgent> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent>('pause_background_agent', { id })
  }
  const body: ControlBackgroundAgentRequest = { action: 'pause' }
  const response = await apiClient.post<BackgroundAgent>(
    API_ENDPOINTS.BACKGROUND_AGENT.CONTROL(id),
    body,
  )
  return response.data
}

/**
 * Resume a paused background agent.
 */
export async function resumeBackgroundAgent(id: string): Promise<BackgroundAgent> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent>('resume_background_agent', { id })
  }
  const body: ControlBackgroundAgentRequest = { action: 'resume' }
  const response = await apiClient.post<BackgroundAgent>(
    API_ENDPOINTS.BACKGROUND_AGENT.CONTROL(id),
    body,
  )
  return response.data
}

/**
 * Get events for a specific background agent.
 * @param backgroundAgentId - The background agent ID
 * @param limit - Optional maximum number of events to return
 */
export async function getBackgroundAgentEvents(
  backgroundAgentId: string,
  limit?: number,
): Promise<BackgroundAgentEvent[]> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgentEvent[]>('get_background_agent_events', {
      taskId: backgroundAgentId,
      limit,
    })
  }
  const url = limit
    ? `${API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS(backgroundAgentId)}?event_limit=${limit}`
    : API_ENDPOINTS.BACKGROUND_AGENT.PROGRESS(backgroundAgentId)
  const response = await apiClient.get<BackgroundProgress>(url)
  return response.data.recent_events
}

/**
 * Get background agents that are ready to run (based on schedule).
 */
export async function getRunnableBackgroundAgents(): Promise<BackgroundAgent[]> {
  if (isTauri()) {
    return tauriInvoke<BackgroundAgent[]>('get_runnable_background_agents')
  }
  const response = await apiClient.get<BackgroundAgent[]>(
    API_ENDPOINTS.BACKGROUND_AGENT.LIST_BY_STATUS('active'),
  )
  const now = Date.now()
  return response.data.filter(
    (backgroundAgent) =>
      backgroundAgent.status === 'active' &&
      (backgroundAgent.next_run_at === null || backgroundAgent.next_run_at <= now),
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
export function createOnceSchedule(runAt: number): BackgroundAgentSchedule {
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
export function createIntervalSchedule(
  intervalMs: number,
  startAt?: number,
): BackgroundAgentSchedule {
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
export function createCronSchedule(expression: string, timezone?: string): BackgroundAgentSchedule {
  return {
    type: 'cron',
    expression,
    timezone: timezone ?? null,
  }
}

/**
 * Format schedule for display
 */
export function formatSchedule(schedule: BackgroundAgentSchedule): string {
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
 * Format background agent status for display.
 */
export function formatBackgroundAgentStatus(status: BackgroundAgentStatus): string {
  const statusMap: Record<BackgroundAgentStatus, string> = {
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
export function getStatusColor(status: BackgroundAgentStatus): string {
  const colorMap: Record<BackgroundAgentStatus, string> = {
    active: 'success',
    paused: 'info',
    running: 'primary',
    completed: 'success',
    failed: 'danger',
  }
  return colorMap[status] || 'default'
}

// ============================================================================
// Background Agent Stream Event Handling
// ============================================================================

/**
 * Listen for all background agent stream events.
 *
 * This registers a listener for all background agent execution events.
 * Use this for global monitoring or when managing multiple concurrent agents.
 *
 * @param callback - Function to call with each event
 * @returns Unlisten function to stop listening
 */
export async function onBackgroundAgentStreamEvent(
  callback: (event: BackgroundAgentStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    // In web mode, return a no-op unlisten function
    console.warn('Background agent stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<BackgroundAgentStreamEvent>(BACKGROUND_AGENT_STREAM_EVENT, (event) => {
    callback(event.payload)
  })
}

/**
 * Listen for stream events for a specific background agent.
 *
 * This filters events to only those belonging to the specified background agent.
 * Use this when monitoring a single background agent execution.
 *
 * @param backgroundAgentId - Background agent ID to filter events
 * @param callback - Function to call with each matching event
 * @returns Unlisten function to stop listening
 */
export async function onBackgroundAgentStreamEventForBackgroundAgent(
  backgroundAgentId: string,
  callback: (event: BackgroundAgentStreamEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('Background agent stream events are only available in Tauri desktop app')
    return () => {}
  }

  return listen<BackgroundAgentStreamEvent>(BACKGROUND_AGENT_STREAM_EVENT, (event) => {
    if (event.payload.task_id === backgroundAgentId) {
      callback(event.payload)
    }
  })
}

/**
 * Compatibility alias for existing callers.
 */
export const onBackgroundAgentStreamEventForAgent = onBackgroundAgentStreamEventForBackgroundAgent

/**
 * Type guard for checking event kind
 */
export function isEventKind<T extends StreamEventKind['type']>(
  event: BackgroundAgentStreamEvent,
  type: T,
): event is BackgroundAgentStreamEvent & { kind: Extract<StreamEventKind, { type: T }> } {
  return event.kind.type === type
}

/**
 * Run a background agent with streaming support.
 *
 * This is a convenience wrapper that invokes the Tauri command to run
 * a background agent with real-time event streaming.
 *
 * @param backgroundAgentId - ID of the background agent to run
 * @returns Promise that resolves when the background agent starts (not when it completes)
 */
export async function runBackgroundAgentStreaming(backgroundAgentId: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('Streaming background agent execution is only available in Tauri desktop app')
  }
  return tauriInvoke<void>('run_background_agent_streaming', { id: backgroundAgentId })
}

/**
 * Get list of currently active (running) background agents.
 *
 * @returns Array of currently running background agents
 */
export async function getActiveBackgroundAgents(): Promise<ActiveBackgroundAgentInfo[]> {
  if (!isTauri()) {
    return []
  }
  const payload = await tauriInvoke<ActiveBackgroundAgentInfoPayload[]>(
    'get_active_background_agents',
  )
  return payload.map((entry) => ({
    background_agent_id: entry.task_id,
    background_agent_name: entry.task_name,
    executor_agent_id: entry.agent_id,
    started_at: entry.started_at,
    execution_mode: entry.execution_mode,
  }))
}

/**
 * Cancel a running background agent.
 *
 * @param backgroundAgentId - ID of the background agent to cancel
 * @returns true if cancellation was requested successfully
 */
export async function cancelBackgroundAgent(backgroundAgentId: string): Promise<boolean> {
  if (!isTauri()) {
    throw new Error('Background agent cancellation is only available in Tauri desktop app')
  }
  return tauriInvoke<boolean>('cancel_background_agent', { taskId: backgroundAgentId })
}
