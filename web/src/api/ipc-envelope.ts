/**
 * Daemon transport envelope types used by the browser HTTP client.
 *
 * These are intentionally hand-authored Web-facing wrappers around the daemon
 * `/api/request` envelope and are not generated from Rust shared models.
 */

import type { ErrorPayload } from '@/types/generated/ErrorPayload'

export type IpcRequest = {
  type: string
  data?: unknown
}

export type IpcResponse =
  | { response_type: 'pong' }
  | { response_type: 'success'; data: unknown }
  | { response_type: 'error'; data: ErrorPayload }
