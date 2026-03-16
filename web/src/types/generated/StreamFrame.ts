import type { ErrorPayload } from './ErrorPayload'
import type { IpcStreamEvent } from './IpcStreamEvent'

export type StreamFrame =
  | { stream_type: 'start'; data: { stream_id: string } }
  | { stream_type: 'ack'; data: { content: string } }
  | { stream_type: 'data'; data: { content: string } }
  | {
      stream_type: 'tool_call'
      data: { id: string; name: string; arguments: unknown }
    }
  | {
      stream_type: 'tool_result'
      data: { id: string; result: string; success: boolean }
    }
  | { stream_type: 'event'; data: { event: IpcStreamEvent } }
  | { stream_type: 'done'; data: { total_tokens?: number | null } }
  | { stream_type: 'error'; data: ErrorPayload }
