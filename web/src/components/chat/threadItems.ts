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
    data: {
      message_id: message.id,
      role: message.role,
      content: message.content,
      timestamp: message.timestamp.toString(),
      execution: message.execution,
      media: message.media ?? null,
      transcript: message.transcript ?? null,
    },
    step: {
      type: 'message',
      name: `${message.role} message`,
      status: 'completed',
      toolId: `message-${message.id}`,
      result: stringifyData({
        message_id: message.id,
        role: message.role,
        content: message.content,
        timestamp: message.timestamp.toString(),
        execution: message.execution,
        media: message.media ?? null,
        transcript: message.transcript ?? null,
      }),
    },
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
    step: {
      type: event.category,
      name: event.tool_call?.tool_name ?? eventTitle(event),
      status: normalizeStepStatus(eventStatus(event) ?? 'completed'),
      toolId: event.id,
      result: stringifyData(safeEvent),
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

function buildExecutionEventItem(event: ExecutionTraceEvent): ThreadItem | null {
  const kind = mapEventKind(event)
  if (!kind) return null

  if (kind === 'message') {
    const message = buildEventMessage(event)
    return {
      ...buildMessageItem(message),
      id: event.id,
      selection: buildEventSelection(event),
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
    body: selection.step?.result ?? null,
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
    step: {
      type: 'child_run_link',
      name: session.title,
      status: normalizeStepStatus(session.status === 'failed' ? 'failed' : 'completed'),
      toolId: `child-run-${session.id}`,
      result: stringifyData({ child_run: session }),
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
    body: selection.step?.result ?? null,
  }
}

export function buildExecutionThreadItems(thread: ExecutionThread | null): ThreadItem[] {
  if (!thread) return []

  const items = thread.timeline.events
    .map(buildExecutionEventItem)
    .filter((item): item is ThreadItem => item !== null)

  for (const child of thread.child_sessions) {
    items.push(buildChildRunItem(child))
  }

  items.sort((left, right) => {
    const leftTime = left.selection?.data?.event && typeof (left.selection.data.event as ExecutionTraceEvent).timestamp === 'number'
      ? (left.selection.data.event as ExecutionTraceEvent).timestamp
      : left.kind === 'child_run_link'
        ? Number((thread.child_sessions.find((session) => `child-run-${session.id}` === left.id)?.started_at ?? 0))
        : 0
    const rightTime = right.selection?.data?.event && typeof (right.selection.data.event as ExecutionTraceEvent).timestamp === 'number'
      ? (right.selection.data.event as ExecutionTraceEvent).timestamp
      : right.kind === 'child_run_link'
        ? Number((thread.child_sessions.find((session) => `child-run-${session.id}` === right.id)?.started_at ?? 0))
        : 0

    if (leftTime !== rightTime) return leftTime - rightTime
    return left.id.localeCompare(right.id)
  })

  return items
}
