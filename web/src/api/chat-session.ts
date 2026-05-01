/**
 * Chat Session API
 *
 * Provides CRUD operations and messaging for workspace chat sessions.
 */

import { requestTyped, streamClient } from './http-client'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { ChatSessionEvent } from '@/types/generated/ChatSessionEvent'

export type { ChatSession, ChatSessionSummary, ChatMessage, ChatSessionEvent }
export type UnlistenFn = () => void

export interface CreateChatSessionRequest {
  agentId: string
  model: string
  name?: string
  skillId?: string
}

export interface UpdateChatSessionRequest {
  agentId?: string
  model?: string
  name?: string
}

export async function createChatSession(request: CreateChatSessionRequest): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'CreateSession',
    data: {
      agent_id: request.agentId,
      model: request.model,
      name: request.name ?? null,
      skill_id: request.skillId ?? null,
    },
  })
}

export async function listChatSessions(): Promise<ChatSession[]> {
  return requestTyped<ChatSession[]>({ type: 'ListFullSessions' })
}

export async function listChatSessionSummaries(): Promise<ChatSessionSummary[]> {
  return requestTyped<ChatSessionSummary[]>({ type: 'ListSessions' })
}

export async function getChatSession(id: string): Promise<ChatSession> {
  return requestTyped<ChatSession>({ type: 'GetSession', data: { id } })
}

export async function updateChatSession(
  id: string,
  updates: UpdateChatSessionRequest,
): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'UpdateSession',
    data: {
      id,
      updates: {
        agentId: updates.agentId ?? null,
        model: updates.model ?? null,
        name: updates.name ?? null,
      },
    },
  })
}

export async function renameChatSession(id: string, name: string): Promise<ChatSession> {
  return requestTyped<ChatSession>({ type: 'RenameSession', data: { id, name } })
}

export async function deleteChatSession(id: string): Promise<boolean> {
  const response = await requestTyped<{ deleted: boolean }>({
    type: 'DeleteSession',
    data: { id },
  })
  return response.deleted
}

export async function archiveChatSession(id: string): Promise<boolean> {
  const response = await requestTyped<{ archived: boolean }>({
    type: 'ArchiveSession',
    data: { id },
  })
  return response.archived
}

export async function rebuildExternalChatSession(id: string): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'RebuildExternalSession',
    data: { id },
  })
}

export async function addChatMessage(
  sessionId: string,
  message: ChatMessage,
): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'AppendMessage',
    data: {
      session_id: sessionId,
      message,
    },
  })
}

export async function sendChatMessage(sessionId: string, content: string): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'AddMessage',
    data: {
      session_id: sessionId,
      role: 'user',
      content,
    },
  })
}

export async function listChatSessionsByAgent(agentId: string): Promise<ChatSession[]> {
  return requestTyped<ChatSession[]>({
    type: 'ListSessionsByAgent',
    data: { agent_id: agentId },
  })
}

export async function listChatSessionsBySkill(skillId: string): Promise<ChatSession[]> {
  return requestTyped<ChatSession[]>({
    type: 'ListSessionsBySkill',
    data: { skill_id: skillId },
  })
}

export async function executeChatSession(sessionId: string): Promise<ChatSession> {
  return requestTyped<ChatSession>({
    type: 'ExecuteChatSession',
    data: {
      session_id: sessionId,
      user_input: null,
    },
  })
}

export async function subscribeSessionEvents(
  callback: (event: ChatSessionEvent) => void,
): Promise<UnlistenFn> {
  const abortController = new AbortController()

  void (async () => {
    try {
      for await (const frame of streamClient(
        { type: 'SubscribeSessionEvents' },
        { signal: abortController.signal },
      )) {
        if (
          frame.stream_type === 'event' &&
          'session' in frame.data.event &&
          frame.data.event.session
        ) {
          callback(frame.data.event.session)
        }
      }
    } catch (error) {
      if (abortController.signal.aborted) {
        return
      }
      console.warn('Session event stream closed unexpectedly', error)
    }
  })()

  return () => abortController.abort()
}
