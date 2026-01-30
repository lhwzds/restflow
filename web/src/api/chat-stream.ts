/**
 * Chat Stream API
 *
 * Provides streaming chat message functionality using Tauri events.
 * Enables real-time token-by-token AI response streaming.
 */

import { isTauri, tauriInvoke } from './config'

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
export async function sendChatMessageStream(
  sessionId: string,
  message: string
): Promise<string> {
  if (isTauri()) {
    return tauriInvoke<string>('send_chat_message_stream', {
      sessionId,
      message,
    })
  }
  // Web API fallback - not implemented for streaming yet
  throw new Error('Streaming not supported in web mode')
}

/**
 * Cancel an active streaming chat response.
 *
 * @param sessionId - Chat session ID
 * @param messageId - Message ID being generated
 */
export async function cancelChatStream(
  sessionId: string,
  messageId: string
): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('cancel_chat_stream', {
      sessionId,
      messageId,
    })
  }
  throw new Error('Streaming not supported in web mode')
}
