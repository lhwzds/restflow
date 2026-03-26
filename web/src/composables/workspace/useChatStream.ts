/**
 * Chat Stream Composable
 *
 * Handles streaming chat responses from the daemon HTTP stream endpoint.
 */

import { ref, computed, onUnmounted, type ComputedRef } from 'vue'
import { cancelChatStream, openChatStream } from '@/api/chat-stream'
import { queryExecutionTraces } from '@/api/execution-traces'
import type { ExecutionTraceEvent } from '@/types/generated/ExecutionTraceEvent'
import type { StreamFrame } from '@/types/generated/StreamFrame'
import type { StepStatus } from '@/types/generated/StepStatus'

export interface StreamState {
  messageId: string | null
  isStreaming: boolean
  content: string
  tokenCount: number
  inputTokens: number
  outputTokens: number
  steps: StreamStep[]
  error: string | null
  startedAt: number | null
  completedAt: number | null
  thinking: string
  acknowledgement: string
}

export interface StreamStep {
  type: string
  name: string
  displayName?: string
  status: StepStatus
  toolId?: string
  arguments?: string
  result?: string
}

function createInitialState(): StreamState {
  return {
    messageId: null,
    isStreaming: false,
    content: '',
    tokenCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    steps: [],
    error: null,
    startedAt: null,
    completedAt: null,
    thinking: '',
    acknowledgement: '',
  }
}

type JsonRecord = Record<string, unknown>

function parseJsonObject(value: string | null | undefined): JsonRecord | null {
  if (!value) return null
  const trimmed = value.trim()
  if (!trimmed) return null
  try {
    const parsed = JSON.parse(trimmed)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as JsonRecord
    }
  } catch {
    // Keep raw rendering when tool payload is not valid JSON.
  }
  return null
}

function formatToolDisplayName(
  toolName: string,
  argsPayload?: string | null,
  resultPayload?: string | null,
): string {
  if (toolName === 'spawn_subagent') {
    const args = parseJsonObject(argsPayload)
    const result = parseJsonObject(resultPayload)
    const agent =
      (typeof args?.agent === 'string' ? args.agent : null) ??
      (typeof result?.agent === 'string' ? result.agent : null)
    const model = typeof args?.model === 'string' ? args.model : null
    const taskId = typeof result?.task_id === 'string' ? result.task_id : null
    const parts: string[] = []
    if (agent) parts.push(agent)
    if (model) parts.push(`@${model}`)
    if (taskId) parts.push(`#${taskId.slice(0, 8)}`)
    if (parts.length > 0) {
      return `spawn_subagent (${parts.join(' ')})`
    }
  }

  if (toolName === 'wait_subagents') {
    const args = parseJsonObject(argsPayload)
    const taskIds = Array.isArray(args?.task_ids) ? args.task_ids.length : 0
    if (taskIds > 0) {
      return `wait_subagents (${taskIds} tasks)`
    }
  }

  if (toolName === 'list_subagents') {
    const result = parseJsonObject(resultPayload)
    const runningCount =
      typeof result?.running_count === 'number' && Number.isFinite(result.running_count)
        ? Math.max(0, Math.floor(result.running_count))
        : null
    if (runningCount !== null) {
      return `list_subagents (${runningCount} running)`
    }
  }

  return toolName
}

export function useChatStream(sessionId: () => string | null) {
  const state = ref<StreamState>(createInitialState())
  let disposed = false
  let streamAbortController: AbortController | null = null

  function markRunningStepsFailed(): void {
    for (const step of state.value.steps) {
      if (step.status === 'running') {
        step.status = 'failed'
      }
    }
  }

  function upsertToolCall(id: string, name: string, args: unknown): void {
    const serializedArgs = args === undefined ? undefined : JSON.stringify(args)
    state.value.steps.push({
      type: 'tool_call',
      name,
      displayName: formatToolDisplayName(name, serializedArgs, null),
      status: 'running',
      toolId: id,
      arguments: serializedArgs,
    })
  }

  function applyToolResult(id: string, result: string, success: boolean): void {
    const step =
      state.value.steps.find((item) => item.toolId === id) ??
      state.value.steps.find((item) => item.status === 'running')

    if (!step) {
      state.value.steps.push({
        type: 'tool_call',
        name: 'tool',
        status: success ? 'completed' : 'failed',
        toolId: id,
        result,
      })
      return
    }

    step.result = result
    step.status = success ? 'completed' : 'failed'
    step.displayName = formatToolDisplayName(step.name, step.arguments, result)
  }

  async function syncPersistedExecutionEvents(turnId: string): Promise<void> {
    const sid = sessionId()
    if (!sid) return

    try {
      const events = await queryExecutionTraces({
        task_id: null,
        run_id: null,
        parent_run_id: null,
        session_id: sid,
        turn_id: turnId,
        agent_id: null,
        category: null,
        source: null,
        from_timestamp: null,
        to_timestamp: null,
        limit: 200,
        offset: 0,
      })
      if (disposed || state.value.messageId !== turnId) return

      const steps = buildStepsFromExecutionEvents(events)
      if (steps.length > 0) {
        state.value.steps = steps
      }
    } catch {
      // Ignore persistence read errors and keep live stream behavior.
    }
  }

  function buildStepsFromExecutionEvents(events: ExecutionTraceEvent[]): StreamStep[] {
    const sortedEvents = [...events].sort(
      (a, b) => a.timestamp - b.timestamp || a.id.localeCompare(b.id),
    )
    const steps: StreamStep[] = []
    const stepByToolId = new Map<string, StreamStep>()

    for (const event of sortedEvents) {
      if (event.category === 'tool_call' && event.tool_call?.phase === 'started') {
        const toolId = event.tool_call.tool_call_id
        const step: StreamStep = {
          type: 'tool_call',
          name: event.tool_call.tool_name ?? 'tool',
          displayName: formatToolDisplayName(
            event.tool_call.tool_name ?? 'tool',
            event.tool_call.input ?? event.tool_call.input_summary ?? undefined,
            null,
          ),
          status: 'running',
          toolId,
          arguments: event.tool_call.input ?? event.tool_call.input_summary ?? undefined,
        }
        stepByToolId.set(toolId, step)
        steps.push(step)
        continue
      }

      if (event.category === 'tool_call' && event.tool_call?.phase === 'completed') {
        const toolId = event.tool_call.tool_call_id
        let step = stepByToolId.get(toolId)
        if (!step) {
          step = {
            type: 'tool_call',
            name: event.tool_call.tool_name ?? 'tool',
            displayName: formatToolDisplayName(
              event.tool_call.tool_name ?? 'tool',
              event.tool_call.input ?? event.tool_call.input_summary ?? undefined,
              event.tool_call.output ?? event.tool_call.error ?? undefined,
            ),
            status: 'running',
            toolId,
          }
          stepByToolId.set(toolId, step)
          steps.push(step)
        }
        step.displayName = formatToolDisplayName(
          step.name,
          step.arguments,
          event.tool_call.output ?? event.tool_call.error ?? undefined,
        )
        step.status = event.tool_call.success === false ? 'failed' : 'completed'
        if (event.tool_call.output) {
          step.result = event.tool_call.output
        } else if (event.tool_call.error) {
          step.result = event.tool_call.error
        }
        continue
      }

      if (
        event.category === 'lifecycle' &&
        (event.lifecycle?.status === 'turn_failed' ||
          event.lifecycle?.status === 'run_failed' ||
          event.lifecycle?.status === 'turn_interrupted' ||
          event.lifecycle?.status === 'run_interrupted')
      ) {
        for (const step of steps) {
          if (step.status === 'running') {
            step.status = 'failed'
          }
        }
      }
    }

    return steps
  }

  async function consumeFrames(frames: AsyncGenerator<StreamFrame>, turnId: string): Promise<void> {
    try {
      for await (const frame of frames) {
        if (disposed || state.value.messageId !== turnId) {
          break
        }

        switch (frame.stream_type) {
          case 'start':
            break
          case 'ack':
            state.value.acknowledgement = frame.data.content
            if (!state.value.content) {
              state.value.content = `${frame.data.content}\n\n`
            }
            break
          case 'data':
            state.value.content += frame.data.content
            break
          case 'tool_call':
            upsertToolCall(frame.data.id, frame.data.name, frame.data.arguments)
            break
          case 'tool_result':
            applyToolResult(frame.data.id, frame.data.result, frame.data.success)
            break
          case 'done':
            state.value.isStreaming = false
            state.value.completedAt = Date.now()
            state.value.tokenCount = frame.data.total_tokens ?? state.value.tokenCount
            await syncPersistedExecutionEvents(turnId)
            return
          case 'error':
            state.value.isStreaming = false
            state.value.completedAt = Date.now()
            state.value.error = frame.data.message
            markRunningStepsFailed()
            await syncPersistedExecutionEvents(turnId)
            return
          case 'event':
            break
        }
      }

      if (state.value.isStreaming && state.value.messageId === turnId) {
        state.value.isStreaming = false
        state.value.completedAt = Date.now()
        await syncPersistedExecutionEvents(turnId)
      }
    } catch (error) {
      if (streamAbortController?.signal.aborted) {
        return
      }
      state.value.isStreaming = false
      state.value.completedAt = Date.now()
      state.value.error = error instanceof Error ? error.message : 'Streaming request failed'
      markRunningStepsFailed()
      await syncPersistedExecutionEvents(turnId)
    }
  }

  async function send(message: string): Promise<string> {
    const sid = sessionId()
    if (!sid) throw new Error('No session ID')
    if (state.value.isStreaming) {
      throw new Error('Streaming response is already in progress')
    }

    streamAbortController?.abort()
    streamAbortController = new AbortController()

    const { streamId, frames } = openChatStream(sid, message, streamAbortController.signal)
    state.value = {
      ...createInitialState(),
      messageId: streamId,
      isStreaming: true,
      startedAt: Date.now(),
    }

    void consumeFrames(frames, streamId)
    void syncPersistedExecutionEvents(streamId)
    return streamId
  }

  async function cancel(): Promise<void> {
    const mid = state.value.messageId
    if (!mid) return

    await cancelChatStream(mid)
    streamAbortController?.abort()
    state.value.isStreaming = false
    state.value.completedAt = Date.now()
  }

  function reset(): void {
    state.value = createInitialState()
  }

  const isStreaming: ComputedRef<boolean> = computed(() => state.value.isStreaming)
  const hasError: ComputedRef<boolean> = computed(() => state.value.error !== null)
  const duration: ComputedRef<number> = computed(() => {
    if (!state.value.startedAt) return 0
    const end = state.value.completedAt ?? Date.now()
    return end - state.value.startedAt
  })
  const tokensPerSecond: ComputedRef<number> = computed(() => {
    const ms = duration.value
    if (ms === 0) return 0
    return (state.value.tokenCount / ms) * 1000
  })

  onUnmounted(() => {
    disposed = true
    streamAbortController?.abort()
    streamAbortController = null
  })

  return {
    state,
    isStreaming,
    hasError,
    duration,
    tokensPerSecond,
    send,
    cancel,
    reset,
  }
}

export type UseChatStreamReturn = ReturnType<typeof useChatStream>
