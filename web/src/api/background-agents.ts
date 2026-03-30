/**
 * Background Agent API
 *
 * Browser-first wrappers around daemon request contracts.
 */

import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { BackgroundAgentConversionResult } from '@/types/generated/BackgroundAgentConversionResult'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { OperationAssessment } from '@/utils/operationAssessment'
import { BackendError, fetchJson, requestOptional, requestTyped } from './http-client'

export type { BackgroundAgent, TaskEvent }

export interface StreamingBackgroundAgentResponse {
  task_id: string
  event_channel: string
  already_running: boolean
}

type DeleteBackgroundAgentResult = {
  id: string
  deleted: boolean
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

export async function deleteBackgroundAgent(
  id: string,
  confirmationToken?: string,
): Promise<boolean> {
  const outcome = await requestTyped<BackgroundAgentCommandOutcome<DeleteBackgroundAgentResult>>({
    type: 'DeleteBackgroundAgent',
    data: {
      id,
      preview: false,
      confirmation_token: confirmationToken ?? null,
    },
  })

  switch (outcome.status) {
    case 'executed':
      return outcome.result.deleted
    case 'confirmation_required':
      throw toAssessmentError(
        428,
        'conflict',
        outcome.assessment,
        'Confirmation required before deleting this background agent.',
      )
    case 'blocked':
      throw toAssessmentError(
        400,
        'validation',
        outcome.assessment,
        'Failed to delete background agent.',
      )
    case 'preview':
      throw toAssessmentError(
        409,
        'conflict',
        outcome.assessment,
        'Preview responses are not supported for direct background-agent deletion.',
      )
  }
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

type BackgroundAgentCommandOutcome<T> =
  | { status: 'preview'; assessment: OperationAssessment }
  | { status: 'blocked'; assessment: OperationAssessment }
  | { status: 'confirmation_required'; assessment: OperationAssessment }
  | { status: 'executed'; result: T }

function firstAssessmentMessage(
  assessment: OperationAssessment,
  fallback: string,
): string {
  return assessment.blockers[0]?.message ?? assessment.warnings[0]?.message ?? fallback
}

function toAssessmentError(
  code: number,
  kind: 'validation' | 'conflict',
  assessment: OperationAssessment,
  fallbackMessage: string,
): BackendError {
  return new BackendError({
    code,
    kind,
    message: firstAssessmentMessage(assessment, fallbackMessage),
    details: { assessment },
  })
}

export async function convertSessionToBackgroundAgent(
  request: ConvertSessionToBackgroundAgentRequest,
): Promise<BackgroundAgent> {
  const outcome = await fetchJson<BackgroundAgentCommandOutcome<BackgroundAgentConversionResult>>(
    '/api/background-agents/convert-session',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    },
  )

  switch (outcome.status) {
    case 'executed':
      return outcome.result.task
    case 'confirmation_required':
      throw toAssessmentError(
        428,
        'conflict',
        outcome.assessment,
        'Confirmation required before converting this session.',
      )
    case 'blocked':
      throw toAssessmentError(
        400,
        'validation',
        outcome.assessment,
        'Failed to convert session to background agent.',
      )
    case 'preview':
      throw toAssessmentError(
        409,
        'conflict',
        outcome.assessment,
        'Preview responses are not supported for direct session conversion.',
      )
  }
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
