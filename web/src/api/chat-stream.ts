/**
 * Chat stream API
 *
 * Web transport wrappers around daemon stream contracts.
 */

import { requestTyped, streamClient } from './http-client'
import type { StreamFrame } from '@/types/generated/StreamFrame'

export interface ChatStreamHandle {
  streamId: string
  frames: AsyncGenerator<StreamFrame>
}

function createStreamId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `stream-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

export function openChatStream(
  sessionId: string,
  message: string,
  signal?: AbortSignal,
): ChatStreamHandle {
  const streamId = createStreamId()
  const frames = streamClient(
    {
      type: 'ExecuteChatSessionStream',
      data: {
        session_id: sessionId,
        user_input: message,
        stream_id: streamId,
      },
    },
    { signal },
  )

  return { streamId, frames }
}

export async function cancelChatStream(streamId: string): Promise<void> {
  await requestTyped<null>({
    type: 'CancelChatSessionStream',
    data: { stream_id: streamId },
  })
}

export async function steerChatStream(sessionId: string, instruction: string): Promise<boolean> {
  const response = await requestTyped<{ steered: boolean }>({
    type: 'SteerChatSessionStream',
    data: {
      session_id: sessionId,
      instruction,
    },
  })
  return response.steered
}
