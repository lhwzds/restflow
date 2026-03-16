import { requestTyped } from './http-client'

export type ToolTraceEventType =
  | 'turn_started'
  | 'tool_call_started'
  | 'tool_call_completed'
  | 'turn_completed'
  | 'turn_failed'
  | 'turn_interrupted'

export interface ToolTrace {
  id: string
  session_id: string
  turn_id: string
  message_id: string | null
  event_type: ToolTraceEventType
  tool_call_id: string | null
  tool_name: string | null
  input: string | null
  output: string | null
  output_ref: string | null
  success: boolean | null
  duration_ms: number | null
  error: string | null
  created_at: number
}

export async function listToolTraces(
  sessionId: string,
  turnId?: string,
  limit?: number,
): Promise<ToolTrace[]> {
  return requestTyped<ToolTrace[]>({
    type: 'ListToolTraces',
    data: {
      session_id: sessionId,
      turn_id: turnId ?? null,
      limit: limit ?? null,
    },
  })
}
