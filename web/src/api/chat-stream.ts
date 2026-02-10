/**
 * Chat Stream API
 *
 * Provides streaming chat message functionality using Tauri events.
 * Enables real-time token-by-token AI response streaming.
 */

import { tauriInvoke } from './tauri-client'

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
  return tauriInvoke<string>('send_chat_message_stream', {
    sessionId,
    message,
  })
}

/**
 * Cancel an active streaming chat response.
 *
 * @param sessionId - Chat session ID
 * @param messageId - Message ID being generated
 */
export async function cancelChatStream(sessionId: string, messageId: string): Promise<void> {
  return tauriInvoke<void>('cancel_chat_stream', {
    sessionId,
    messageId,
  })
}
