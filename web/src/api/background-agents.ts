/**
 * Background Agent API
 *
 * Thin wrappers around Tauri commands for background agent management.
 */

import { invokeCommand } from './tauri-client'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { TaskEvent } from '@/types/generated/TaskEvent'

export type { BackgroundAgent, TaskEvent }

/** Response from running a background agent with streaming */
export interface StreamingBackgroundAgentResponse {
  task_id: string
  event_channel: string
  already_running: boolean
}

/** List all background agents */
export async function listBackgroundAgents(): Promise<BackgroundAgent[]> {
  return invokeCommand('listBackgroundAgents')
}

/** Pause a background agent */
export async function pauseBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return invokeCommand('pauseBackgroundAgent', id)
}

/** Resume a paused background agent */
export async function resumeBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return invokeCommand('resumeBackgroundAgent', id)
}

/** Stop a running background agent */
export async function stopBackgroundAgent(taskId: string): Promise<boolean> {
  return invokeCommand('stopBackgroundAgent', taskId)
}

/** Run a background agent immediately with streaming */
export async function runBackgroundAgentStreaming(
  id: string,
): Promise<StreamingBackgroundAgentResponse> {
  return invokeCommand('runBackgroundAgentStreaming', id)
}

/** Steer a running task with a new instruction */
export async function steerTask(taskId: string, instruction: string): Promise<boolean> {
  return invokeCommand('steerTask', taskId, instruction)
}

/** Get event history for a task */
export async function getBackgroundAgentEvents(
  taskId: string,
  limit?: number,
): Promise<TaskEvent[]> {
  return invokeCommand('getBackgroundAgentEvents', taskId, limit ?? null)
}

/** Get the stream event channel name (also activates the Rust bridge) */
export async function getBackgroundAgentStreamEventName(): Promise<string> {
  return invokeCommand('getBackgroundAgentStreamEventName')
}

/** Get the heartbeat event channel name */
export async function getHeartbeatEventName(): Promise<string> {
  return invokeCommand('getHeartbeatEventName')
}

/** Delete a background agent */
export async function deleteBackgroundAgent(id: string): Promise<boolean> {
  return invokeCommand('deleteBackgroundAgent', id)
}

/** Request payload for converting a session to background agent */
export interface ConvertSessionToBackgroundAgentRequest {
  session_id: string
  name?: string
  input?: string
  run_now?: boolean
}

/** Request payload for updating an existing background agent. */
export interface UpdateBackgroundAgentRequest {
  name?: string
  description?: string
  agent_id?: string
  chat_session_id?: string
  input?: string
  input_template?: string
  timeout_secs?: number
}

/** Convert a chat session into a background agent */
export async function convertSessionToBackgroundAgent(
  request: ConvertSessionToBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  return invokeCommand('convertSessionToBackgroundAgent', request)
}

/** Update an existing background agent */
export async function updateBackgroundAgent(
  id: string,
  request: UpdateBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  return invokeCommand('updateBackgroundAgent', id, request)
}

/** List memory sessions for a memory namespace (agent ID) */
export async function listMemorySessions(agentId: string): Promise<MemorySession[]> {
  return invokeCommand('listMemorySessions', agentId)
}

/** List memory chunks for a given session */
export async function listMemoryChunksForSession(sessionId: string): Promise<MemoryChunk[]> {
  return invokeCommand('listMemoryChunksForSession', sessionId)
}

/** List memory chunks by tag (used for task:<background-agent-id>) */
export async function listMemoryChunksByTag(tag: string, limit?: number): Promise<MemoryChunk[]> {
  const response = await invokeCommand<{ items: MemoryChunk[]; total: number }>(
    'listMemoryChunksByTag',
    tag,
    limit ?? null,
  )
  return response.items
}
