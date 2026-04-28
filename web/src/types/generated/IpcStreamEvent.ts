import type { ChatSessionEvent } from './ChatSessionEvent'
import type { TaskStreamEvent } from './TaskStreamEvent'

export type IpcStreamEvent = { task: TaskStreamEvent } | { session: ChatSessionEvent }
