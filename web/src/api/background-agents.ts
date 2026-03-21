/**
 * Background Agent API
 *
 * Browser-first wrappers around daemon request contracts.
 */

import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import { fetchJson, requestOptional, requestTyped } from './http-client'

export type { BackgroundAgent, TaskEvent }

export interface StreamingBackgroundAgentResponse {
  task_id: string
  event_channel: string
  already_running: boolean
}

export async function listBackgroundAgents(): Promise<BackgroundAgent[]> {
  return requestTyped<BackgroundAgent[]>({
    type: 'ListBackgroundAgents',
    data: { status: null },
  })
}

export async function getBackgroundAgent(id: string): Promise<BackgroundAgent | null> {
  return requestOptional<BackgroundAgent>({
    type: 'GetBackgroundAgent',
    data: { id },
  })
}

export async function pauseBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return requestTyped<BackgroundAgent>({
    type: 'ControlBackgroundAgent',
    data: { id, action: 'pause', preview: false, confirmation_token: null },
  })
}

export async function resumeBackgroundAgent(id: string): Promise<BackgroundAgent> {
  return requestTyped<BackgroundAgent>({
    type: 'ControlBackgroundAgent',
    data: { id, action: 'resume', preview: false, confirmation_token: null },
  })
}

export async function stopBackgroundAgent(taskId: string): Promise<boolean> {
  await requestTyped({
    type: 'ControlBackgroundAgent',
    data: { id: taskId, action: 'stop', preview: false, confirmation_token: null },
  })
  return true
}

export async function runBackgroundAgentStreaming(
  id: string,
  confirmationToken?: string,
): Promise<StreamingBackgroundAgentResponse> {
  const agent = await requestTyped<BackgroundAgent>({
    type: 'ControlBackgroundAgent',
    data: {
      id,
      action: 'run_now',
      preview: false,
      confirmation_token: confirmationToken ?? null,
    },
  })

  return {
    task_id: agent.id,
    event_channel: '/api/stream',
    already_running: false,
  }
}

export async function steerTask(taskId: string, instruction: string): Promise<boolean> {
  const response = await requestTyped<{ steered: boolean }>({
    type: 'SendBackgroundAgentMessage',
    data: { id: taskId, message: instruction, source: 'user' },
  })
  return response.steered
}

export async function getBackgroundAgentEvents(
  taskId: string,
  limit?: number,
): Promise<TaskEvent[]> {
  return requestTyped<TaskEvent[]>({
    type: 'GetBackgroundAgentHistory',
    data: { id: taskId, limit: limit ?? null },
  })
}

export async function getBackgroundAgentStreamEventName(): Promise<string> {
  return 'background-agent:stream'
}

export async function getHeartbeatEventName(): Promise<string> {
  return 'background-agent:heartbeat'
}

export async function deleteBackgroundAgent(id: string): Promise<boolean> {
  const response = await requestTyped<{ deleted: boolean }>({
    type: 'DeleteBackgroundAgent',
    data: { id },
  })
  return response.deleted
}

export interface ConvertSessionToBackgroundAgentRequest {
  session_id: string
  name?: string
  input?: string
  run_now?: boolean
  confirmation_token?: string
}

export interface UpdateBackgroundAgentRequest {
  name?: string
  description?: string
  agent_id?: string
  chat_session_id?: string
  input?: string
  input_template?: string
  timeout_secs?: number
  preview?: boolean
  confirmation_token?: string
}

export async function convertSessionToBackgroundAgent(
  request: ConvertSessionToBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  return fetchJson<BackgroundAgent>('/api/background-agents/convert-session', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
}

export async function updateBackgroundAgent(
  id: string,
  request: UpdateBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  return requestTyped<BackgroundAgent>({
    type: 'UpdateBackgroundAgent',
    data: {
      id,
      patch: {
        ...request,
        preview: undefined,
        confirmation_token: undefined,
      },
      preview: request.preview ?? false,
      confirmation_token: request.confirmation_token ?? null,
    },
  })
}

export async function listMemorySessions(agentId: string): Promise<MemorySession[]> {
  return requestTyped<MemorySession[]>({
    type: 'ListMemorySessions',
    data: { agent_id: agentId },
  })
}

export async function listMemoryChunksForSession(sessionId: string): Promise<MemoryChunk[]> {
  return requestTyped<MemoryChunk[]>({
    type: 'ListMemoryBySession',
    data: { session_id: sessionId },
  })
}

export async function listMemoryChunksByTag(tag: string, limit?: number): Promise<MemoryChunk[]> {
  const chunks = await requestTyped<MemoryChunk[]>({
    type: 'ListMemory',
    data: { agent_id: null, tag },
  })
  const effectiveLimit = limit ?? 50
  return chunks.slice(0, effectiveLimit)
}
