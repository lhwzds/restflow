/**
 * Background Agent API
 *
 * Thin wrappers around Tauri commands for background agent management.
 */

import { tauriInvoke } from './tauri-client'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { TaskEvent } from '@/types/generated/TaskEvent'

export type { BackgroundAgent, TaskEvent }

interface MemoryListResponse<T> {
  items: T[]
  total: number
}

/** Response from running a background agent with streaming */
export interface StreamingBackgroundAgentResponse {
  task_id: string
  event_channel: string
  already_running: boolean
}

/** Info about an actively running background agent */
export interface ActiveBackgroundAgentInfo {
  task_id: string
  task_name: string
  agent_id: string
  started_at: number
  execution_mode: string
}

/** List all background agents */
export async function listBackgroundAgents(): Promise<BackgroundAgent[]> {
  return tauriInvoke<BackgroundAgent[]>('list_background_agents')
}

/** Get a single background agent by ID */
export async function getBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return tauriInvoke<BackgroundAgent>('get_background_agent', { id })
}

/** Pause a background agent */
export async function pauseBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return tauriInvoke<BackgroundAgent>('pause_background_agent', { id })
}

/** Resume a paused background agent */
export async function resumeBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return tauriInvoke<BackgroundAgent>('resume_background_agent', { id })
}

/** Cancel a running background agent */
export async function cancelBackgroundAgent(taskId: string): Promise<boolean> {
  return tauriInvoke<boolean>('cancel_background_agent', { task_id: taskId })
}

/** Run a background agent immediately with streaming */
export async function runBackgroundAgentStreaming(
  id: string,
): Promise<StreamingBackgroundAgentResponse> {
  return tauriInvoke<StreamingBackgroundAgentResponse>('run_background_agent_streaming', { id })
}

/** Steer a running task with a new instruction */
export async function steerTask(taskId: string, instruction: string): Promise<boolean> {
  return tauriInvoke<boolean>('steer_task', { task_id: taskId, instruction })
}

/** Get event history for a task */
export async function getBackgroundAgentEvents(
  taskId: string,
  limit?: number,
): Promise<TaskEvent[]> {
  return tauriInvoke<TaskEvent[]>('get_background_agent_events', { task_id: taskId, limit })
}

/** Get the stream event channel name (also activates the Rust bridge) */
export async function getBackgroundAgentStreamEventName(): Promise<string> {
  return tauriInvoke<string>('get_background_agent_stream_event_name')
}

/** Get the heartbeat event channel name */
export async function getHeartbeatEventName(): Promise<string> {
  return tauriInvoke<string>('get_heartbeat_event_name')
}

/** Get currently active/running background agents */
export async function getActiveBackgroundAgents(): Promise<ActiveBackgroundAgentInfo[]> {
  return tauriInvoke<ActiveBackgroundAgentInfo[]>('get_active_background_agents')
}

/** Delete a background agent */
export async function deleteBackgroundAgent(id: string): Promise<boolean> {
  return tauriInvoke<boolean>('delete_background_agent', { id })
}

/** List memory sessions for a memory namespace (agent ID) */
export async function listMemorySessions(agentId: string): Promise<MemorySession[]> {
  return tauriInvoke<MemorySession[]>('list_memory_sessions', { agent_id: agentId })
}

/** List memory chunks for a given session */
export async function listMemoryChunksForSession(sessionId: string): Promise<MemoryChunk[]> {
  return tauriInvoke<MemoryChunk[]>('list_memory_chunks_for_session', { session_id: sessionId })
}

/** List memory chunks by tag (used for task:<background-agent-id>) */
export async function listMemoryChunksByTag(tag: string, limit?: number): Promise<MemoryChunk[]> {
  const response = await tauriInvoke<MemoryListResponse<MemoryChunk>>('list_memory_chunks_by_tag', {
    tag,
    limit,
  })
  return response.items
}
