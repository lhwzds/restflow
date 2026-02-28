import { describe, expect, it } from 'vitest'
import { buildBackgroundTimelineMessages } from '../backgroundTimeline'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'

function createEvent(overrides: Partial<TaskEvent> = {}): TaskEvent {
  return {
    id: overrides.id ?? 'event-1',
    task_id: overrides.task_id ?? 'task-1',
    event_type: overrides.event_type ?? 'started',
    timestamp: overrides.timestamp ?? 1000,
    message: overrides.message ?? null,
    output: overrides.output ?? null,
    tokens_used: overrides.tokens_used ?? null,
    cost_usd: overrides.cost_usd ?? null,
    duration_ms: overrides.duration_ms ?? null,
    subflow_path: overrides.subflow_path ?? [],
  }
}

function createChunk(overrides: Partial<MemoryChunk> = {}): MemoryChunk {
  return {
    id: 'chunk-1',
    agent_id: 'agent-1',
    session_id: 'session-1',
    content: 'chunk content',
    content_hash: 'hash-1',
    source: { type: 'task_execution', task_id: 'task-1' },
    created_at: 3000,
    tags: [],
    token_count: null,
    ...overrides,
  }
}

describe('buildBackgroundTimelineMessages', () => {
  it('maps output events to assistant messages', () => {
    const messages = buildBackgroundTimelineMessages({
      events: [createEvent({ output: 'final output', event_type: 'completed' })],
      memoryChunks: [],
    })

    expect(messages).toHaveLength(1)
    const first = messages[0]
    expect(first).toBeDefined()
    expect(first!.role).toBe('assistant')
    expect(first!.content).toBe('final output')
  })

  it('maps non-output events to system messages with summary', () => {
    const messages = buildBackgroundTimelineMessages({
      events: [
        createEvent({
          event_type: 'completed',
          duration_ms: 1500,
          tokens_used: 42,
          message: 'run finished',
        }),
      ],
      memoryChunks: [],
    })

    const first = messages[0]
    expect(first).toBeDefined()
    expect(first!.role).toBe('system')
    expect(first!.content).toContain('Completed')
    expect(first!.content).toContain('1.5s')
    expect(first!.content).toContain('42 tokens')
    expect(first!.content).toContain('run finished')
  })

  it('sorts events and memory chunks by timestamp', () => {
    const messages = buildBackgroundTimelineMessages({
      events: [createEvent({ id: 'event-1', timestamp: 2000, output: 'event output' })],
      memoryChunks: [createChunk({ id: 'chunk-1', created_at: 1000, content: 'memory output' })],
    })

    expect(messages).toHaveLength(2)
    const first = messages[0]
    const second = messages[1]
    expect(first).toBeDefined()
    expect(second).toBeDefined()
    expect(first!.id).toBe('bg-memory-chunk-1')
    expect(second!.id).toBe('bg-event-output-event-1')
  })
})
