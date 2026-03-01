/**
 * Chat Session API
 *
 * Provides CRUD operations and messaging for workspace chat sessions.
 */

import { invokeCommand } from './tauri-client'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatMessage } from '@/types/generated/ChatMessage'

// Re-export types for convenience
export type { ChatSession, ChatSessionSummary, ChatMessage }

/**
 * Request payload for creating a chat session
 */
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

/**
 * Create a new chat session.
 */
export async function createChatSession(request: CreateChatSessionRequest): Promise<ChatSession> {
  return invokeCommand(
    'createChatSession',
    request.agentId,
    request.model,
    request.name ?? null,
    request.skillId ?? null,
  )
}

/**
 * List all chat sessions.
 */
export async function listChatSessions(): Promise<ChatSession[]> {
  return invokeCommand('listChatSessions')
}

/**
 * List chat session summaries.
 */
export async function listChatSessionSummaries(): Promise<ChatSessionSummary[]> {
  return invokeCommand('listChatSessionSummaries')
}

/**
 * Get a chat session by ID.
 */
export async function getChatSession(id: string): Promise<ChatSession> {
  return invokeCommand('getChatSession', id)
}

/**
 * Update a chat session.
 */
export async function updateChatSession(
  id: string,
  updates: UpdateChatSessionRequest,
): Promise<ChatSession> {
  return invokeCommand('updateChatSession', id, {
    agentId: updates.agentId ?? null,
    model: updates.model ?? null,
    name: updates.name ?? null,
  })
}

/**
 * Rename a chat session.
 */
export async function renameChatSession(id: string, name: string): Promise<ChatSession> {
  return invokeCommand('renameChatSession', id, name)
}

/**
 * Delete a chat session.
 */
export async function deleteChatSession(id: string): Promise<boolean> {
  return invokeCommand('deleteChatSession', id)
}

/**
 * Archive a chat session.
 */
export async function archiveChatSession(id: string): Promise<boolean> {
  return invokeCommand('archiveChatSession', id)
}

/**
 * Rebuild an externally managed session (Telegram/Discord/Slack) with a fresh history.
 */
export async function rebuildExternalChatSession(id: string): Promise<ChatSession> {
  return invokeCommand('rebuildExternalChatSession', id)
}

/**
 * Add a message to a chat session.
 */
export async function addChatMessage(
  sessionId: string,
  message: ChatMessage,
): Promise<ChatSession> {
  return invokeCommand('addChatMessage', sessionId, message)
}

/**
 * Send a chat message and trigger agent response.
 */
export async function sendChatMessage(sessionId: string, content: string): Promise<ChatSession> {
  return invokeCommand('sendChatMessage', sessionId, content)
}

/**
 * List chat sessions for a specific agent.
 */
export async function listChatSessionsByAgent(agentId: string): Promise<ChatSession[]> {
  return invokeCommand('listChatSessionsByAgent', agentId)
}

/**
 * List chat sessions for a specific skill.
 */
export async function listChatSessionsBySkill(skillId: string): Promise<ChatSession[]> {
  return invokeCommand('listChatSessionsBySkill', skillId)
}

/**
 * Trigger assistant response generation for a chat session.
 */
export async function executeChatSession(sessionId: string): Promise<ChatSession> {
  return invokeCommand('executeChatSession', sessionId)
}

/**
 * Session change event from the daemon (e.g. Telegram message added).
 */
export interface ChatSessionEvent {
  type: 'Created' | 'Updated' | 'MessageAdded' | 'Deleted'
  session_id: string
  source?: string
}

/**
 * Subscribe to real-time session change events from the daemon.
 * Call once at startup; returns an unlisten function.
 */
export async function subscribeSessionEvents(
  callback: (event: ChatSessionEvent) => void,
): Promise<UnlistenFn> {
  const eventName = await invokeCommand<string>('getSessionChangeEventName')
  return listen<ChatSessionEvent>(eventName, (e) => callback(e.payload))
}
