import type { ErrorPayload } from './ErrorPayload'

export type IpcResponse =
  | { response_type: 'pong' }
  | { response_type: 'success'; data: unknown }
  | { response_type: 'error'; data: ErrorPayload }
