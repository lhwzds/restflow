import { describe, expect, it } from 'vitest'
import { hasPersistedOutputForTask, shouldShowLiveStreamBubble } from '../streamVisibility'
import type { TaskEvent } from '@/types/generated/TaskEvent'

function createTaskEvent(overrides: Partial<TaskEvent> = {}): TaskEvent {
  return {
    id: overrides.id ?? 'event-1',
    task_id: overrides.task_id ?? 'task-1',
    event_type: overrides.event_type ?? 'started',
    timestamp: overrides.timestamp ?? Date.now(),
    message: overrides.message ?? null,
    output: overrides.output ?? null,
    tokens_used: overrides.tokens_used ?? null,
    cost_usd: overrides.cost_usd ?? null,
    duration_ms: overrides.duration_ms ?? null,
    subflow_path: overrides.subflow_path ?? [],
  }
}

describe('streamVisibility', () => {
  it('detects persisted output for the same task', () => {
    const events = [
      createTaskEvent({ task_id: 'task-1', output: null }),
      createTaskEvent({ id: 'event-2', task_id: 'task-1', output: 'final answer' }),
    ]

    expect(hasPersistedOutputForTask(events, 'task-1')).toBe(true)
    expect(hasPersistedOutputForTask(events, 'task-2')).toBe(false)
  })

  it('shows live stream while task is running', () => {
    expect(
      shouldShowLiveStreamBubble({
        streamTaskId: 'task-1',
        isStreaming: true,
        outputText: '',
        events: [],
      }),
    ).toBe(true)
  })

  it('hides live stream after completion when persisted output exists', () => {
    const events = [createTaskEvent({ task_id: 'task-1', output: 'persisted output' })]

    expect(
      shouldShowLiveStreamBubble({
        streamTaskId: 'task-1',
        isStreaming: false,
        outputText: 'live output buffer',
        events,
      }),
    ).toBe(false)
  })

  it('keeps live stream visible when persisted output has not arrived yet', () => {
    const events = [createTaskEvent({ task_id: 'task-2', output: 'other task output' })]

    expect(
      shouldShowLiveStreamBubble({
        streamTaskId: 'task-1',
        isStreaming: false,
        outputText: 'live output buffer',
        events,
      }),
    ).toBe(true)
  })
})
