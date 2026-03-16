import type { ChatSessionEvent } from './ChatSessionEvent'
import type { TaskStreamEvent } from './TaskStreamEvent'

export type IpcStreamEvent =
  | { background_agent: TaskStreamEvent }
  | { session: ChatSessionEvent }
