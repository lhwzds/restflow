import type { StreamStep } from '@/composables/workspace/useChatStream'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { ChatRole } from '@/types/generated/ChatRole'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ExecutionStepInfo } from '@/types/generated/ExecutionStepInfo'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import type { ExecutionTraceEvent } from '@/types/generated/ExecutionTraceEvent'
import type { StepStatus } from '@/types/generated/StepStatus'

export type ThreadItemKind =
  | 'message'
  | 'tool_call'
  | 'llm_call'
  | 'model_switch'
  | 'lifecycle'
  | 'log_record'
  | 'child_run_link'

export type ThreadSelectionKind = 'message' | 'step' | 'event' | 'child_run'

export interface ThreadSelection {
  id: string
  kind: ThreadSelectionKind
  title: string
  data: Record<string, unknown>
  step?: StreamStep
  toolName?: string
}

export interface ThreadItem {
  id: string
  kind: ThreadItemKind
  title: string
  summary?: string | null
  body?: string | null
  role?: ChatMessage['role']
  status?: string | null
  durationLabel?: string | null
  timestampLabel?: string | null
  message?: ChatMessage
  selection?: ThreadSelection
  expandable: boolean
}

interface ThreadEnvelope {
  item: ThreadItem
  sortTime: number
  sortId: string
  sequence: number
}

function normalizeStepStatus(status: string): StepStatus {
  switch (status) {
    case 'completed':
    case 'failed':
    case 'pending':
    case 'running':
      return status
    default:
      return 'completed'
  }
}

function formatDurationLabel(durationMs: bigint | number | null | undefined): string | null {
  if (durationMs == null) return null
  return `${(Number(durationMs) / 1000).toFixed(1)}s`
}

function formatTimestampLabel(timestamp: number | bigint | null | undefined): string | null {
  if (timestamp == null) return null
  const value = typeof timestamp === 'bigint' ? Number(timestamp) : timestamp
  if (!Number.isFinite(value)) return null
  return new Date(value).toLocaleString()
}

function stringifyData(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, current) => (typeof current === 'bigint' ? current.toString() : current),
    2,
  )
}

function buildMessageSelectionData(message: ChatMessage): Record<string, unknown> {
  return {
    message_id: message.id,
    role: message.role,
    content: message.content,
    timestamp: message.timestamp.toString(),
    execution: message.execution,
    media: message.media ?? null,
    transcript: message.transcript ?? null,
  }
}

function buildPersistedSelection(messageId: string, step: ExecutionStepInfo, index: number): ThreadSelection {
  const metadata = {
    persisted_execution_step: true,
    message_id: messageId,
    step_index: index,
    step_type: step.step_type,
    name: step.name,
    status: step.status,
    duration_ms: step.duration_ms == null ? null : Number(step.duration_ms),
  }

  const result = {
    ...metadata,
    note:
      step.step_type === 'tool_call'
        ? 'Detailed persisted tool payload is not available yet.'
        : 'Persisted execution step summary.',
  }

  return {
    id: `persisted-${messageId}-${index}`,
    kind: 'step',
    title: step.step_type === 'tool_call' ? `${step.name} details` : `${step.step_type}: ${step.name}`,
    toolName: step.name,
    data: result,
    step: {
      type: step.step_type === 'tool_call' ? 'tool_call' : step.step_type,
      name: step.name,
      displayName: step.name,
      status: normalizeStepStatus(step.status),
      toolId: `persisted-${messageId}-${index}`,
      arguments: JSON.stringify(metadata),
      result: stringifyData(result),
    },
  }
}

function buildPersistedStepItem(message: ChatMessage, step: ExecutionStepInfo, index: number): ThreadItem {
  const selection = buildPersistedSelection(message.id, step, index)
  const kind = step.step_type === 'tool_call' ? 'tool_call' : toNonToolKind(step.step_type)

  return {
    id: selection.id,
    kind,
    title: step.name,
    summary: selection.step?.result ? null : undefined,
    body: selection.step?.result ?? null,
    status: step.status,
    durationLabel: formatDurationLabel(step.duration_ms),
    timestampLabel: formatTimestampLabel(message.timestamp),
    selection,
    expandable: true,
  }
}

function buildMessageSelection(message: ChatMessage): ThreadSelection {
  return {
    id: `message-${message.id}`,
    kind: 'message',
    title: `${message.role} message`,
    data: buildMessageSelectionData(message),
  }
}

function buildMessageItem(message: ChatMessage): ThreadItem {
  return {
    id: message.id,
    kind: 'message',
    title: message.role,
    role: message.role,
    message,
    timestampLabel: formatTimestampLabel(message.timestamp),
    selection: buildMessageSelection(message),
    expandable: false,
  }
}

function buildStreamStepItem(step: StreamStep, index: number): ThreadItem {
  const kind = step.type === 'tool_call' ? 'tool_call' : toNonToolKind(step.type)
  const selection: ThreadSelection = {
    id: step.toolId ?? `stream-${index}`,
    kind: 'step',
    title: step.type === 'tool_call' ? `${step.name || 'Tool'} details` : `${step.type}: ${step.name || 'details'}`,
    toolName: step.name,
    data: {
      step_type: step.type,
      name: step.name,
      status: step.status,
      display_name: step.displayName,
      arguments: step.arguments ?? null,
      result: step.result ?? null,
    },
    step,
  }

  return {
    id: selection.id,
    kind,
    title: step.displayName || step.name || step.type,
    body: step.result ?? null,
    status: step.status,
    selection,
    expandable: !!step.result,
  }
}

function buildStreamingAssistantItem(content: string): ThreadItem {
  const message: ChatMessage = {
    id: 'streaming-assistant',
    role: 'assistant',
    content,
    timestamp: BigInt(Date.now()),
    execution: null,
  }

  return buildMessageItem(message)
}

function toSortTime(timestamp: number | bigint | null | undefined): number {
  if (timestamp == null) return 0
  const value = typeof timestamp === 'bigint' ? Number(timestamp) : timestamp
  return Number.isFinite(value) ? value : 0
}

function toNonToolKind(type: string): ThreadItemKind {
  switch (type) {
    case 'llm_call':
      return 'llm_call'
    case 'model_switch':
      return 'model_switch'
    case 'lifecycle':
      return 'lifecycle'
    case 'log_record':
      return 'log_record'
    default:
      return 'lifecycle'
  }
}

export function buildChatThreadItems(input: {
  messages: ChatMessage[]
  steps?: StreamStep[]
  isStreaming?: boolean
  streamContent?: string
}): ThreadItem[] {
  const items: ThreadItem[] = []

  for (const message of input.messages) {
    if (message.role === 'assistant' && message.execution?.steps?.length) {
      message.execution.steps.forEach((step, index) => {
        items.push(buildPersistedStepItem(message, step, index))
      })
    }
    items.push(buildMessageItem(message))
  }

  if (input.steps?.length) {
    input.steps.forEach((step, index) => {
      items.push(buildStreamStepItem(step, index))
    })
  }

  if (input.streamContent) {
    items.push(buildStreamingAssistantItem(input.streamContent))
  }

  return items
}

function eventTitle(event: ExecutionTraceEvent): string {
  switch (event.category) {
    case 'tool_call':
      return event.tool_call?.tool_name ?? 'Tool call'
    case 'llm_call':
      return event.llm_call?.model ? `LLM call · ${event.llm_call.model}` : 'LLM call'
    case 'model_switch':
      return event.model_switch
        ? `${event.model_switch.from_model} → ${event.model_switch.to_model}`
        : 'Model switch'
    case 'lifecycle':
      return event.lifecycle?.status ?? 'Lifecycle'
    case 'message':
      return event.message?.role ? `${event.message.role} message` : 'Message'
    case 'log_record':
      return event.log_record?.level ? `Log · ${event.log_record.level}` : 'Log record'
    default:
      return event.category
  }
}

function eventSummary(event: ExecutionTraceEvent): string | null {
  switch (event.category) {
    case 'tool_call':
      return event.tool_call?.error ?? event.tool_call?.input_summary ?? event.tool_call?.output_ref ?? null
    case 'llm_call':
      return [
        event.llm_call?.total_tokens != null ? `${event.llm_call.total_tokens} tokens` : null,
        event.llm_call?.duration_ms != null ? `${event.llm_call.duration_ms} ms` : null,
        event.llm_call?.cost_usd != null ? `$${event.llm_call.cost_usd.toFixed(4)}` : null,
      ]
        .filter(Boolean)
        .join(' · ')
    case 'model_switch':
      return event.model_switch?.reason ?? null
    case 'lifecycle':
      return event.lifecycle?.message ?? event.lifecycle?.error ?? null
    case 'message':
      return event.message?.content_preview ?? null
    case 'log_record':
      return event.log_record?.message ?? null
    default:
      return null
  }
}

function eventStatus(event: ExecutionTraceEvent): string | null {
  switch (event.category) {
    case 'tool_call':
      return event.tool_call?.phase ?? null
    case 'lifecycle':
      return event.lifecycle?.status ?? null
    default:
      return null
  }
}

function buildEventSelection(event: ExecutionTraceEvent): ThreadSelection {
  const safeEvent = {
    ...event,
    subflow_path: [...event.subflow_path],
  }

  return {
    id: event.id,
    kind: 'event',
    title: eventTitle(event),
    toolName: event.tool_call?.tool_name ?? undefined,
    data: {
      event: safeEvent,
    },
  }
}

function buildCanonicalEventSelection(
  event: ExecutionTraceEvent,
  message: ChatMessage | null,
): ThreadSelection {
  if (!message) {
    return buildEventSelection(event)
  }

  const safeEvent = {
    ...event,
    subflow_path: [...event.subflow_path],
  }

  return {
    id: event.id,
    kind: 'event',
    title: eventTitle(event),
    toolName: event.tool_call?.tool_name ?? undefined,
    data: {
      event: safeEvent,
      message: buildMessageSelectionData(message),
    },
  }
}

function mapEventKind(event: ExecutionTraceEvent): ThreadItemKind | null {
  switch (event.category) {
    case 'message':
      return 'message'
    case 'tool_call':
      return 'tool_call'
    case 'llm_call':
      return 'llm_call'
    case 'model_switch':
      return 'model_switch'
    case 'lifecycle':
      return 'lifecycle'
    case 'log_record':
      return 'log_record'
    default:
      return null
  }
}

function buildEventMessage(event: ExecutionTraceEvent): ChatMessage {
  const role = (event.message?.role ?? 'system') as ChatRole
  return {
    id: event.id,
    role,
    content: event.message?.content_preview ?? '',
    timestamp: BigInt(event.timestamp),
    execution: null,
  }
}

function buildExecutionEventItem(
  event: ExecutionTraceEvent,
  matchedMessage: ChatMessage | null = null,
): ThreadItem | null {
  const kind = mapEventKind(event)
  if (!kind) return null

  if (kind === 'message') {
    const message = matchedMessage ?? buildEventMessage(event)
    return {
      ...buildMessageItem(message),
      id: event.id,
      selection: buildCanonicalEventSelection(event, matchedMessage),
      timestampLabel: formatTimestampLabel(event.timestamp),
    }
  }

  const selection = buildEventSelection(event)
  const summary = eventSummary(event)
  return {
    id: event.id,
    kind,
    title: eventTitle(event),
    summary,
    body: stringifyData(selection.data),
    status: eventStatus(event),
    durationLabel:
      event.llm_call?.duration_ms != null
        ? `${event.llm_call.duration_ms} ms`
        : null,
    timestampLabel: formatTimestampLabel(event.timestamp),
    selection,
    expandable: true,
  }
}

function buildChildRunItem(session: ExecutionSessionSummary): ThreadItem {
  const selection: ThreadSelection = {
    id: `child-run-${session.id}`,
    kind: 'child_run',
    title: session.title,
    data: {
      child_run: session,
    },
  }

  return {
    id: selection.id,
    kind: 'child_run_link',
    title: session.title,
    summary: session.subtitle ?? session.run_id ?? session.session_id ?? null,
    status: session.status,
    timestampLabel: formatTimestampLabel(session.started_at ?? session.updated_at),
    selection,
    expandable: true,
    body: stringifyData(selection.data),
  }
}

function normalizeComparableText(value: string): string {
  return value.replace(/\s+/g, ' ').trim()
}

function previewMatchesMessage(message: ChatMessage, preview: string | null | undefined): boolean {
  if (!preview) return true

  const normalizedPreview = normalizeComparableText(preview).replace(/[.…]+$/g, '')
  if (!normalizedPreview) return true

  const normalizedContent = normalizeComparableText(message.content)
  if (!normalizedContent) return false

  return (
    normalizedContent.includes(normalizedPreview) ||
    normalizedContent.startsWith(normalizedPreview) ||
    normalizedPreview.includes(normalizedContent.slice(0, normalizedPreview.length))
  )
}

function resolveMessageMatches(
  events: ExecutionTraceEvent[],
  messages: ChatMessage[],
): {
  matchedByEventId: Map<string, ChatMessage>
  unmatchedMessages: ChatMessage[]
} {
  const matchedByEventId = new Map<string, ChatMessage>()
  const usedMessageIndexes = new Set<number>()
  const sortedMessages = [...messages].sort((left, right) => {
    const delta = toSortTime(left.timestamp) - toSortTime(right.timestamp)
    return delta !== 0 ? delta : left.id.localeCompare(right.id)
  })

  let cursor = 0

  function findMatchIndex(role: string, preview: string | null | undefined): number {
    const scan = (start: number, requirePreview: boolean): number => {
      for (let index = start; index < sortedMessages.length; index += 1) {
        if (usedMessageIndexes.has(index)) continue
        const candidate = sortedMessages[index]
        if (!candidate) continue
        if (candidate.role !== role) continue
        if (!requirePreview || previewMatchesMessage(candidate, preview)) {
          return index
        }
      }
      return -1
    }

    const withPreviewAfterCursor = scan(cursor, true)
    if (withPreviewAfterCursor >= 0) return withPreviewAfterCursor

    const withPreviewAnywhere = scan(0, true)
    if (withPreviewAnywhere >= 0) return withPreviewAnywhere

    const sameRoleAfterCursor = scan(cursor, false)
    if (sameRoleAfterCursor >= 0) return sameRoleAfterCursor

    return scan(0, false)
  }

  for (const event of events) {
    if (event.category !== 'message' || !event.message?.role) continue

    const matchIndex = findMatchIndex(event.message.role, event.message.content_preview)
    if (matchIndex < 0) continue

    usedMessageIndexes.add(matchIndex)
    matchedByEventId.set(event.id, sortedMessages[matchIndex]!)
    cursor = matchIndex + 1
  }

  const unmatchedMessages = sortedMessages.filter((_message, index) => !usedMessageIndexes.has(index))
  return {
    matchedByEventId,
    unmatchedMessages,
  }
}

function buildChildRunEnvelope(session: ExecutionSessionSummary): ThreadEnvelope {
  return {
    item: buildChildRunItem(session),
    sortTime: toSortTime(session.started_at ?? session.updated_at),
    sortId: `child-run-${session.id}`,
    sequence: Number.MAX_SAFE_INTEGER,
  }
}

function appendLiveOverlays(items: ThreadItem[], steps: StreamStep[] | undefined, streamContent: string | undefined) {
  if (steps?.length) {
    steps.forEach((step, index) => {
      items.push(buildStreamStepItem(step, index))
    })
  }

  if (streamContent) {
    items.push(buildStreamingAssistantItem(streamContent))
  }
}

function isOptimisticMessage(message: ChatMessage): boolean {
  return message.id.startsWith('optimistic-')
}

function buildTransientMessageItems(messages: ChatMessage[]): ThreadItem[] {
  return [...messages]
    .filter(isOptimisticMessage)
    .sort((left, right) => {
      const delta = toSortTime(left.timestamp) - toSortTime(right.timestamp)
      return delta !== 0 ? delta : left.id.localeCompare(right.id)
    })
    .map((message) => buildMessageItem(message))
}

export function buildExecutionThreadItems(thread: ExecutionThread | null): ThreadItem[] {
  if (!thread) return []

  const envelopes: ThreadEnvelope[] = thread.timeline.events
    .map((event, index) => {
      const item = buildExecutionEventItem(event)
      if (!item) return null
      return {
        item,
        sortTime: toSortTime(event.timestamp),
        sortId: event.id,
        sequence: index,
      }
    })
    .filter((entry): entry is ThreadEnvelope => entry !== null)

  for (const child of thread.child_sessions) {
    envelopes.push(buildChildRunEnvelope(child))
  }

  envelopes.sort((left, right) => {
    if (left.sortTime !== right.sortTime) return left.sortTime - right.sortTime
    if (left.sequence !== right.sequence) return left.sequence - right.sequence
    return left.sortId.localeCompare(right.sortId)
  })

  return envelopes.map((entry) => entry.item)
}

export function buildSessionThreadItems(input: {
  thread: ExecutionThread | null
  messages: ChatMessage[]
  steps?: StreamStep[]
  streamContent?: string
}): ThreadItem[] {
  if (!input.thread) {
    const hasPersistedMessages = input.messages.some((message) => !isOptimisticMessage(message))
    if (hasPersistedMessages) {
      return buildChatThreadItems({
        messages: input.messages,
        steps: input.steps,
        streamContent: input.streamContent,
      })
    }

    const items = buildTransientMessageItems(input.messages)
    appendLiveOverlays(items, input.steps, input.streamContent)
    return items
  }

  const canonicalEvents = [...input.thread.timeline.events]
  const { matchedByEventId, unmatchedMessages } = resolveMessageMatches(canonicalEvents, input.messages)
  const envelopes: ThreadEnvelope[] = []

  for (const event of canonicalEvents) {
    const item = buildExecutionEventItem(event, matchedByEventId.get(event.id) ?? null)
    if (!item) continue
    envelopes.push({
      item,
      sortTime: toSortTime(event.timestamp),
      sortId: event.id,
      sequence: envelopes.length,
    })
  }

  for (const message of unmatchedMessages) {
    envelopes.push({
      item: buildMessageItem(message),
      sortTime: toSortTime(message.timestamp),
      sortId: `message-${message.id}`,
      sequence: envelopes.length,
    })
  }

  for (const child of input.thread.child_sessions) {
    envelopes.push(buildChildRunEnvelope(child))
  }

  envelopes.sort((left, right) => {
    if (left.sortTime !== right.sortTime) return left.sortTime - right.sortTime
    if (left.sequence !== right.sequence) return left.sequence - right.sequence
    return left.sortId.localeCompare(right.sortId)
  })

  const items = envelopes.map((entry) => entry.item)
  appendLiveOverlays(items, input.steps, input.streamContent)
  return items
}
