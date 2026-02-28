/**
 * Chat Session API
 *
 * Provides CRUD operations and messaging for workspace chat sessions.
 */

import { tauriInvoke } from './tauri-client'
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
  return tauriInvoke<ChatSession>('create_chat_session', {
    agentId: request.agentId,
    model: request.model,
    name: request.name,
    skillId: request.skillId,
  })
}

/**
 * List all chat sessions.
 */
export async function listChatSessions(): Promise<ChatSession[]> {
  return tauriInvoke<ChatSession[]>('list_chat_sessions')
}

/**
 * List chat session summaries.
 */
export async function listChatSessionSummaries(): Promise<ChatSessionSummary[]> {
  return tauriInvoke<ChatSessionSummary[]>('list_chat_session_summaries')
}

/**
 * Get a chat session by ID.
 */
export async function getChatSession(id: string): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('get_chat_session', { id })
}

/**
 * Update a chat session.
 */
export async function updateChatSession(
  id: string,
  updates: UpdateChatSessionRequest,
): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('update_chat_session', { sessionId: id, updates })
}

/**
 * Rename a chat session.
 */
export async function renameChatSession(id: string, name: string): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('rename_chat_session', { id, name })
}

/**
 * Delete a chat session.
 */
export async function deleteChatSession(id: string): Promise<boolean> {
  return tauriInvoke<boolean>('delete_chat_session', { id })
}

/**
 * Add a message to a chat session.
 */
export async function addChatMessage(
  sessionId: string,
  message: ChatMessage,
): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('add_chat_message', { sessionId, message })
}

/**
 * Send a chat message and trigger agent response.
 */
export async function sendChatMessage(sessionId: string, content: string): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('send_chat_message', { sessionId, content })
}

/**
 * List chat sessions for a specific agent.
 */
export async function listChatSessionsByAgent(agentId: string): Promise<ChatSession[]> {
  return tauriInvoke<ChatSession[]>('list_chat_sessions_by_agent', { agentId })
}

/**
 * List chat sessions for a specific skill.
 */
export async function listChatSessionsBySkill(skillId: string): Promise<ChatSession[]> {
  return tauriInvoke<ChatSession[]>('list_chat_sessions_by_skill', { skillId })
}

/**
 * Trigger assistant response generation for a chat session.
 */
export async function executeChatSession(sessionId: string): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('execute_chat_session', { sessionId })
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
  const eventName = await tauriInvoke<string>('get_session_change_event_name')
  return listen<ChatSessionEvent>(eventName, (e) => callback(e.payload))
}
