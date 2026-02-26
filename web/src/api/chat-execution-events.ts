import { tauriInvoke } from './tauri-client'

export type ChatExecutionEventType =
  | 'turn_started'
  | 'tool_call_started'
  | 'tool_call_completed'
  | 'turn_completed'
  | 'turn_failed'
  | 'turn_cancelled'

export interface ChatExecutionEvent {
  id: string
  session_id: string
  turn_id: string
  message_id: string | null
  event_type: ChatExecutionEventType
  tool_call_id: string | null
  tool_name: string | null
  input: string | null
  output: string | null
  success: boolean | null
  duration_ms: number | null
  error: string | null
  created_at: number
}

export async function listChatExecutionEvents(
  sessionId: string,
  turnId?: string,
  limit?: number,
): Promise<ChatExecutionEvent[]> {
  return tauriInvoke<ChatExecutionEvent[]>('list_chat_execution_events', {
    sessionId,
    turnId,
    limit,
  })
}
