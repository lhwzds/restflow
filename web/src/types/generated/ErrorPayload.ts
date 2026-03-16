import type { ErrorKind } from './ErrorKind'

export type ErrorPayload = {
  code: number
  kind: ErrorKind
  message: string
  details?: unknown | null
  retryable?: boolean | null
}
