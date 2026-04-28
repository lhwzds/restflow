import type { ChatSessionEvent } from './ChatSessionEvent'
import type { TaskStreamEvent } from './TaskStreamEvent'

export type IpcStreamEvent =
  | { task: TaskStreamEvent }
  | { background_agent: TaskStreamEvent }
  | { session: ChatSessionEvent }
