import type { TaskEvent } from '@/types/generated/TaskEvent'

export function hasPersistedOutputForTask(events: TaskEvent[], taskId: string): boolean {
  return events.some(
    (event) =>
      event.task_id === taskId &&
      typeof event.output === 'string' &&
      event.output.trim().length > 0,
  )
}

export function shouldShowLiveStreamBubble(params: {
  streamTaskId: string | null
  isStreaming: boolean
  outputText: string
  events: TaskEvent[]
}): boolean {
  const { streamTaskId, isStreaming, outputText, events } = params

  if (!streamTaskId) return false
  if (isStreaming) return true
  if (!outputText.trim()) return false

  return !hasPersistedOutputForTask(events, streamTaskId)
}
