/**
 * Chat Stream Composable
 *
 * Handles streaming chat responses from AI, providing real-time
 * token updates, status tracking, and cancellation support.
 */

import { ref, computed, onUnmounted, type ComputedRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { sendChatMessageStream, cancelChatStream } from '@/api/chat-stream'
import { listChatExecutionEvents, type ChatExecutionEvent } from '@/api/chat-execution-events'
import type { ChatStreamEvent } from '@/types/generated/ChatStreamEvent'
import type { ChatStreamKind } from '@/types/generated/ChatStreamKind'
import type { StepStatus } from '@/types/generated/StepStatus'

/**
 * State of an active chat stream
 */
export interface StreamState {
  /** Message ID being generated */
  messageId: string | null
  /** Whether streaming is active */
  isStreaming: boolean
  /** Accumulated response content */
  content: string
  /** Approximate token count */
  tokenCount: number
  /** Input tokens used */
  inputTokens: number
  /** Output tokens generated */
  outputTokens: number
  /** Execution steps */
  steps: StreamStep[]
  /** Error message if failed */
  error: string | null
  /** Stream start timestamp */
  startedAt: bigint | null
  /** Stream completion timestamp */
  completedAt: bigint | null
  /** Thinking/reasoning content */
  thinking: string
}

/**
 * An execution step during streaming
 */
export interface StreamStep {
  type: string
  name: string
  status: StepStatus
  /** Tool call ID for correlating start/end events */
  toolId?: string
  /** Tool call arguments JSON (populated on tool_call_start) */
  arguments?: string
  /** Tool call result JSON (populated on tool_call_end) */
  result?: string
}

/**
 * Create initial stream state
 */
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
  }
}

/**
 * Composable for handling streaming chat responses
 *
 * @param sessionId - Getter for the current session ID
 * @returns Stream state and control methods
 */
export function useChatStream(sessionId: () => string | null) {
  const state = ref<StreamState>(createInitialState())
  let unlistenFn: UnlistenFn | null = null
  let disposed = false

  function buildStepsFromExecutionEvents(events: ChatExecutionEvent[]): StreamStep[] {
    const sortedEvents = [...events].sort(
      (a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id),
    )
    const steps: StreamStep[] = []
    const stepByToolId = new Map<string, StreamStep>()

    for (const event of sortedEvents) {
      if (event.event_type === 'tool_call_started') {
        const toolId = event.tool_call_id ?? event.id
        const step: StreamStep = {
          type: 'tool_call',
          name: event.tool_name ?? 'tool',
          status: 'running',
          toolId,
          arguments: event.input ?? undefined,
        }
        stepByToolId.set(toolId, step)
        steps.push(step)
        continue
      }

      if (event.event_type === 'tool_call_completed') {
        const toolId = event.tool_call_id ?? event.id
        let step = stepByToolId.get(toolId)
        if (!step) {
          step = {
            type: 'tool_call',
            name: event.tool_name ?? 'tool',
            status: 'running',
            toolId,
          }
          stepByToolId.set(toolId, step)
          steps.push(step)
        }
        step.status = event.success === false ? 'failed' : 'completed'
        if (event.output) {
          step.result = event.output
        } else if (event.error) {
          step.result = event.error
        }
        continue
      }

      if (event.event_type === 'turn_failed' || event.event_type === 'turn_cancelled') {
        for (const step of steps) {
          if (step.status === 'running') {
            step.status = 'failed'
          }
        }
      }
    }

    return steps
  }

  async function syncPersistedExecutionEvents(turnId: string): Promise<void> {
    const sid = sessionId()
    if (!sid) return

    try {
      const events = await listChatExecutionEvents(sid, turnId, 200)
      if (disposed || state.value.messageId !== turnId) return

      const steps = buildStepsFromExecutionEvents(events)
      if (steps.length > 0) {
        state.value.steps = steps
      }
    } catch {
      // Ignore persistence read errors and keep live stream behavior.
    }
  }

  /**
   * Set up the Tauri event listener for chat stream events
   */
  async function setupListener(): Promise<void> {
    if (unlistenFn || disposed) return

    const unlisten = await listen<ChatStreamEvent>('chat:stream', (event) => {
      const data = event.payload
      const currentSessionId = sessionId()

      // Only process events for the current session
      if (data.session_id !== currentSessionId) return

      handleEvent(data)
    })

    // Component was unmounted while awaiting listen
    if (disposed) {
      unlisten()
      return
    }

    unlistenFn = unlisten
  }

  /**
   * Handle a chat stream event
   */
  function handleEvent(event: ChatStreamEvent): void {
    const kind = event.kind as ChatStreamKind
    if (state.value.messageId && event.message_id !== state.value.messageId) return

    // Handle different event types based on the 'type' discriminator
    if ('type' in kind) {
      switch (kind.type) {
        case 'started':
          state.value = {
            ...createInitialState(),
            messageId: event.message_id,
            isStreaming: true,
            startedAt: event.timestamp,
          }
          break

        case 'token':
          if ('text' in kind && 'token_count' in kind) {
            state.value.content += kind.text
            state.value.tokenCount = kind.token_count
          }
          break

        case 'thinking':
          if ('content' in kind) {
            state.value.thinking += kind.content
          }
          break

        case 'tool_call_start':
          if ('tool_name' in kind) {
            state.value.steps.push({
              type: 'tool_call',
              name: kind.tool_name,
              status: 'running',
              toolId: kind.tool_id,
              arguments: kind.arguments,
            })
          }
          break

        case 'tool_call_end':
          if ('tool_id' in kind) {
            // Find step by tool_id for accurate correlation, fallback to first running
            const step =
              state.value.steps.find((s) => s.toolId === kind.tool_id) ||
              state.value.steps.find((s) => s.status === 'running')
            if (step && 'success' in kind) {
              step.status = kind.success ? 'completed' : 'failed'
              if ('result' in kind) {
                step.result = kind.result
              }
            }
          }
          break

        case 'step':
          if ('step_type' in kind && 'name' in kind && 'status' in kind) {
            const existingStep = state.value.steps.find((s) => s.name === kind.name)
            if (existingStep) {
              existingStep.status = kind.status
            } else {
              state.value.steps.push({
                type: kind.step_type,
                name: kind.name,
                status: kind.status,
              })
            }
          }
          break

        case 'usage':
          if ('input_tokens' in kind && 'output_tokens' in kind) {
            state.value.inputTokens = kind.input_tokens
            state.value.outputTokens = kind.output_tokens
          }
          break

        case 'completed':
          if ('full_content' in kind && 'total_tokens' in kind) {
            state.value.isStreaming = false
            state.value.content = kind.full_content
            state.value.tokenCount = kind.total_tokens
            state.value.completedAt = event.timestamp
            void syncPersistedExecutionEvents(event.message_id)
          }
          break

        case 'failed':
          if ('error' in kind) {
            state.value.isStreaming = false
            state.value.error = kind.error
            if ('partial_content' in kind && kind.partial_content) {
              state.value.content = kind.partial_content
            }
            void syncPersistedExecutionEvents(event.message_id)
          }
          break

        case 'cancelled':
          state.value.isStreaming = false
          if ('partial_content' in kind && kind.partial_content) {
            state.value.content = kind.partial_content
          }
          void syncPersistedExecutionEvents(event.message_id)
          break
      }
    }
  }

  /**
   * Send a message with streaming response
   *
   * @param message - User message content
   * @returns Message ID of the generated response
   */
  async function send(message: string): Promise<string> {
    const sid = sessionId()
    if (!sid) throw new Error('No session ID')

    await setupListener()
    state.value = createInitialState()
    state.value.isStreaming = true

    try {
      const messageId = await sendChatMessageStream(sid, message)
      state.value.messageId = messageId
      void syncPersistedExecutionEvents(messageId)
      return messageId
    } catch (error) {
      state.value.isStreaming = false
      state.value.error = error instanceof Error ? error.message : 'Failed to send'
      throw error
    }
  }

  /**
   * Cancel the current streaming response
   */
  async function cancel(): Promise<void> {
    const sid = sessionId()
    const mid = state.value.messageId
    if (!sid || !mid) return

    await cancelChatStream(sid, mid)
  }

  /**
   * Reset the stream state
   */
  function reset(): void {
    state.value = createInitialState()
  }

  // Computed properties
  const isStreaming: ComputedRef<boolean> = computed(() => state.value.isStreaming)
  const hasError: ComputedRef<boolean> = computed(() => state.value.error !== null)
  const duration: ComputedRef<number> = computed(() => {
    if (!state.value.startedAt) return 0
    const end = state.value.completedAt ?? BigInt(Date.now())
    return Number(end - state.value.startedAt)
  })
  const tokensPerSecond: ComputedRef<number> = computed(() => {
    const ms = duration.value
    if (ms === 0) return 0
    return (state.value.tokenCount / ms) * 1000
  })

  // Cleanup on unmount
  onUnmounted(() => {
    disposed = true
    if (unlistenFn) {
      unlistenFn()
      unlistenFn = null
    }
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
