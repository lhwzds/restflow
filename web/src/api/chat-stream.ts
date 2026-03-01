/**
 * Chat Stream API
 *
 * Provides streaming chat message functionality using Tauri events.
 * Enables real-time token-by-token AI response streaming.
 */

import { invokeCommand } from './tauri-client'

/**
 * Send a chat message with streaming response.
 *
 * The response will be streamed via Tauri events (chat:stream).
 * Use useChatStream composable to handle the stream events.
 *
 * @param sessionId - Chat session ID
 * @param message - User message content
 * @returns Message ID for the generated response
 */
export async function sendChatMessageStream(sessionId: string, message: string): Promise<string> {
  return invokeCommand('sendChatMessageStream', sessionId, message)
}

/**
 * Cancel an active streaming chat response.
 *
 * @param sessionId - Chat session ID
 * @param messageId - Message ID being generated
 */
export async function cancelChatStream(sessionId: string, messageId: string): Promise<void> {
  await invokeCommand('cancelChatStream', sessionId, messageId)
}

/**
 * Send a steering instruction to the currently running stream for a session.
 *
 * Returns false when no active stream is steerable.
 */
export async function steerChatStream(sessionId: string, instruction: string): Promise<boolean> {
  return invokeCommand('steerChatStream', sessionId, instruction)
}
