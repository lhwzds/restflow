/**
 * Chat Session API
 *
 * Provides CRUD operations and messaging for workspace chat sessions.
 * Supports both Tauri (desktop) and HTTP API (web) backends.
 */

import { apiClient, isTauri, tauriInvoke } from './config'
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
 *
 * @param request - Session creation parameters
 * @returns The created chat session
 */
export async function createChatSession(request: CreateChatSessionRequest): Promise<ChatSession> {
  if (isTauri()) {
    // Tauri v2 uses camelCase for parameter names by default
    return tauriInvoke<ChatSession>('create_chat_session', {
      agentId: request.agentId,
      model: request.model,
      name: request.name,
      skillId: request.skillId,
    })
  }
  const response = await apiClient.post<ChatSession>('/api/chat-sessions', request)
  return response.data
}

/**
 * List all chat sessions.
 *
 * Returns full session objects including messages.
 * For listing without messages, use listChatSessionSummaries().
 */
export async function listChatSessions(): Promise<ChatSession[]> {
  if (isTauri()) {
    return tauriInvoke<ChatSession[]>('list_chat_sessions')
  }
  const response = await apiClient.get<ChatSession[]>('/api/chat-sessions')
  return response.data
}

/**
 * List chat session summaries.
 *
 * More efficient than listChatSessions when full message history isn't needed.
 */
export async function listChatSessionSummaries(): Promise<ChatSessionSummary[]> {
  if (isTauri()) {
    return tauriInvoke<ChatSessionSummary[]>('list_chat_session_summaries')
  }
  const response = await apiClient.get<ChatSessionSummary[]>('/api/chat-sessions/summaries')
  return response.data
}

/**
 * Get a chat session by ID.
 *
 * @param id - Session ID
 * @returns The chat session with full message history
 */
export async function getChatSession(id: string): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('get_chat_session', { id })
  }
  const response = await apiClient.get<ChatSession>(`/api/chat-sessions/${id}`)
  return response.data
}

/**
 * Update a chat session.
 *
 * @param id - Session ID
 * @param updates - Fields to update
 * @returns The updated chat session
 */
export async function updateChatSession(
  id: string,
  updates: UpdateChatSessionRequest,
): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('update_chat_session', { sessionId: id, updates })
  }
  const response = await apiClient.patch<ChatSession>(`/api/chat-sessions/${id}`, updates)
  return response.data
}

/**
 * Rename a chat session.
 *
 * @param id - Session ID
 * @param name - New session name
 * @returns The updated chat session
 */
export async function renameChatSession(id: string, name: string): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('rename_chat_session', { id, name })
  }
  const response = await apiClient.patch<ChatSession>(`/api/chat-sessions/${id}`, { name })
  return response.data
}

/**
 * Delete a chat session.
 *
 * @param id - Session ID
 * @returns True if deleted successfully
 */
export async function deleteChatSession(id: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('delete_chat_session', { id })
  }
  await apiClient.delete(`/api/chat-sessions/${id}`)
  return true
}

/**
 * Add a message to a chat session.
 *
 * Use this for adding user messages. For full send + response flow,
 * use sendChatMessage or streaming response events.
 *
 * @param sessionId - Session ID
 * @param message - The message to add
 * @returns The updated chat session
 */
export async function addChatMessage(
  sessionId: string,
  message: ChatMessage,
): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('add_chat_message', { sessionId, message })
  }
  const response = await apiClient.post<ChatSession>(
    `/api/chat-sessions/${sessionId}/messages`,
    message,
  )
  return response.data
}

/**
 * Send a chat message and trigger agent response.
 *
 * This is a convenience method that adds the user message and triggers
 * assistant response generation. For streaming responses, use
 * addChatMessage + response events.
 *
 * @param sessionId - Session ID
 * @param content - Message content
 * @returns The updated chat session (may not include assistant response yet)
 */
export async function sendChatMessage(sessionId: string, content: string): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('send_chat_message', { sessionId, content })
  }
  const response = await apiClient.post<ChatSession>(`/api/chat-sessions/${sessionId}/send`, {
    content,
  })
  return response.data
}

/**
 * List chat sessions for a specific agent.
 *
 * @param agentId - Agent ID
 * @returns Sessions associated with the agent
 */
export async function listChatSessionsByAgent(agentId: string): Promise<ChatSession[]> {
  if (isTauri()) {
    return tauriInvoke<ChatSession[]>('list_chat_sessions_by_agent', { agentId })
  }
  const response = await apiClient.get<ChatSession[]>(
    `/api/chat-sessions?agent_id=${encodeURIComponent(agentId)}`,
  )
  return response.data
}

/**
 * List chat sessions for a specific skill.
 *
 * @param skillId - Skill ID
 * @returns Sessions associated with the skill
 */
export async function listChatSessionsBySkill(skillId: string): Promise<ChatSession[]> {
  if (isTauri()) {
    return tauriInvoke<ChatSession[]>('list_chat_sessions_by_skill', { skillId })
  }
  const response = await apiClient.get<ChatSession[]>(
    `/api/chat-sessions?skill_id=${encodeURIComponent(skillId)}`,
  )
  return response.data
}

/**
 * Get the count of chat sessions.
 *
 * @returns Number of chat sessions
 */
export async function countChatSessions(): Promise<number> {
  if (isTauri()) {
    return tauriInvoke<number>('get_chat_session_count')
  }
  const response = await apiClient.get<{ count: number }>('/api/chat-sessions/count')
  return response.data.count
}

/**
 * Delete chat sessions older than the specified number of days.
 *
 * @param olderThanDays - Delete sessions not updated in this many days
 * @returns Number of sessions deleted
 */
export async function cleanupOldChatSessions(olderThanDays: number): Promise<number> {
  if (isTauri()) {
    const olderThanMs = Date.now() - olderThanDays * 24 * 60 * 60 * 1000
    return tauriInvoke<number>('clear_old_chat_sessions', {
      older_than_ms: Math.floor(olderThanMs),
    })
  }
  const response = await apiClient.delete<{ deleted: number }>(
    `/api/chat-sessions/cleanup?older_than_days=${olderThanDays}`,
  )
  return response.data.deleted
}

/**
 * Trigger assistant response generation for a chat session.
 *
 * This triggers response generation for the session using the latest user
 * message as input, and saves the assistant response to the session.
 *
 * @param sessionId - Chat session ID
 * @returns The updated chat session with the assistant response
 */
export async function executeChatSession(sessionId: string): Promise<ChatSession> {
  if (isTauri()) {
    return tauriInvoke<ChatSession>('execute_chat_session', { sessionId })
  }
  // HTTP API fallback - not implemented yet
  throw new Error('Assistant response generation is not supported in web mode')
}
