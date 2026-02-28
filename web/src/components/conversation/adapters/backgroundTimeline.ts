import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { TaskEvent } from '@/types/generated/TaskEvent'

function toTimestamp(value: number): bigint {
  const normalized = Number.isFinite(value) ? Math.max(0, Math.floor(value)) : Date.now()
  return BigInt(normalized)
}

function formatDuration(ms: number | null): string | null {
  if (ms == null) return null
  return `${(ms / 1000).toFixed(1)}s`
}

function formatEventType(type: string): string {
  return type.charAt(0).toUpperCase() + type.slice(1).replace(/_/g, ' ')
}

function eventSummary(event: TaskEvent): string | null {
  const parts: string[] = []
  const duration = formatDuration(event.duration_ms)
  if (duration) parts.push(duration)
  if (event.tokens_used != null) parts.push(`${event.tokens_used} tokens`)
  if (event.cost_usd != null) parts.push(`$${event.cost_usd.toFixed(4)}`)
  return parts.length > 0 ? parts.join(' · ') : null
}

function mapEventToMessage(event: TaskEvent): ChatMessage {
  if (event.output && event.output.trim().length > 0) {
    return {
      id: `bg-event-output-${event.id}`,
      role: 'assistant',
      content: event.output,
      timestamp: toTimestamp(event.timestamp),
      execution: null,
    }
  }

  const heading = formatEventType(event.event_type)
  const summary = eventSummary(event)
  const summaryLine = summary ? `\n${summary}` : ''
  const messageLine = event.message ? `\n\n${event.message}` : ''

  return {
    id: `bg-event-meta-${event.id}`,
    role: 'system',
    content: `**${heading}**${summaryLine}${messageLine}`,
    timestamp: toTimestamp(event.timestamp),
    execution: null,
  }
}

function mapMemoryToMessage(chunk: MemoryChunk): ChatMessage {
  const title = new Date(chunk.created_at).toLocaleString()
  return {
    id: `bg-memory-${chunk.id}`,
    role: 'system',
    content: `**Memory Snapshot · ${title}**\n\n${chunk.content}`,
    timestamp: toTimestamp(chunk.created_at),
    execution: null,
  }
}

export function buildBackgroundTimelineMessages(input: {
  events: TaskEvent[]
  memoryChunks: MemoryChunk[]
}): ChatMessage[] {
  const { events, memoryChunks } = input
  const items = [...events.map(mapEventToMessage), ...memoryChunks.map(mapMemoryToMessage)]

  items.sort((a, b) => {
    const diff = Number(a.timestamp - b.timestamp)
    if (diff !== 0) return diff
    return a.id.localeCompare(b.id)
  })

  return items
}
