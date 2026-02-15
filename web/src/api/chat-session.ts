/**
 * Chat Session API
 *
 * Provides CRUD operations and messaging for workspace chat sessions.
 */

import { tauriInvoke } from './tauri-client'
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
 * Get the count of chat sessions.
 */
export async function countChatSessions(): Promise<number> {
  return tauriInvoke<number>('get_chat_session_count')
}

/**
 * Delete chat sessions older than the specified number of days.
 */
export async function cleanupOldChatSessions(olderThanDays: number): Promise<number> {
  const olderThanMs = Date.now() - olderThanDays * 24 * 60 * 60 * 1000
  return tauriInvoke<number>('clear_old_chat_sessions', {
    older_than_ms: Math.floor(olderThanMs),
  })
}

/**
 * Trigger assistant response generation for a chat session.
 */
export async function executeChatSession(sessionId: string): Promise<ChatSession> {
  return tauriInvoke<ChatSession>('execute_chat_session', { sessionId })
}
