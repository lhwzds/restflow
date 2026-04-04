/**
 * Task API
 *
 * Browser-first wrappers around daemon request contracts.
 */

import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { Task } from '@/types/generated/Task'
import type { TaskConversionResult } from '@/types/generated/TaskConversionResult'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import { requestOptional, requestTyped } from './http-client'

export type { Task } from '@/types/generated/Task'
export type { TaskConversionResult } from '@/types/generated/TaskConversionResult'
export type { TaskEvent } from '@/types/generated/TaskEvent'
export type { TaskMessage } from '@/types/generated/TaskMessage'
export type { TaskProgress } from '@/types/generated/TaskProgress'

type DeleteTaskResult = {
  id: string
  deleted: boolean
}

type SteerTaskResult = {
  steered: boolean
}

export async function listTasks(): Promise<Task[]> {
  return requestTyped<Task[]>({
    type: 'ListTasks',
    data: { status: null },
  })
}

export async function getTask(id: string): Promise<Task | null> {
  return requestOptional<Task>({
    type: 'GetTask',
    data: { id },
  })
}

export async function pauseTask(id: string): Promise<Task> {
  return requestTyped<Task>({
    type: 'ControlTask',
    data: { id, action: 'pause' },
  })
}

export async function resumeTask(id: string): Promise<Task> {
  return requestTyped<Task>({
    type: 'ControlTask',
    data: { id, action: 'resume' },
  })
}

export async function stopTask(taskId: string): Promise<Task> {
  return requestTyped<Task>({
    type: 'ControlTask',
    data: { id: taskId, action: 'stop' },
  })
}

export async function runTaskNow(id: string): Promise<Task> {
  return requestTyped<Task>({
    type: 'ControlTask',
    data: { id, action: 'run_now' },
  })
}

export async function steerTask(taskId: string, instruction: string): Promise<SteerTaskResult> {
  return requestTyped<SteerTaskResult>({
    type: 'SendTaskMessage',
    data: { id: taskId, message: instruction, source: 'user' },
  })
}

export async function getTaskEvents(
  taskId: string,
  limit?: number,
): Promise<TaskEvent[]> {
  const events = await requestTyped<TaskEvent[]>({
    type: 'GetTaskHistory',
    data: { id: taskId },
  })
  if (typeof limit === 'number' && limit >= 0) {
    return events.slice(0, limit)
  }
  return events
}

export async function getTaskStreamEventName(): Promise<string> {
  // Transport event names remain legacy until the daemon/browser stream channel is renamed.
  return 'background-agent:stream'
}

export async function getHeartbeatEventName(): Promise<string> {
  // Transport event names remain legacy until the daemon/browser stream channel is renamed.
  return 'background-agent:heartbeat'
}

export async function deleteTask(id: string): Promise<DeleteTaskResult> {
  return requestTyped<DeleteTaskResult>({
    type: 'DeleteTask',
    data: { id },
  })
}

export interface CreateTaskFromSessionRequest {
  session_id: string
  name?: string
  input?: string
  run_now?: boolean
}

export interface UpdateTaskRequest {
  name?: string
  description?: string
  agent_id?: string
  chat_session_id?: string
  input?: string
  input_template?: string
  timeout_secs?: number
}

export async function createTaskFromSession(
  request: CreateTaskFromSessionRequest,
): Promise<TaskConversionResult> {
  return requestTyped<TaskConversionResult>({
    type: 'CreateTaskFromSession',
    data: { request },
  })
}

export async function updateTask(
  id: string,
  request: UpdateTaskRequest,
): Promise<Task> {
  return requestTyped<Task>({
    type: 'UpdateTask',
    data: {
      id,
      patch: request,
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
