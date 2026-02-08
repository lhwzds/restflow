/**
 * Chat Stream Composable
 *
 * Handles streaming chat responses from AI, providing real-time
 * token updates, status tracking, and cancellation support.
 */

import { ref, computed, onUnmounted, type ComputedRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { sendChatMessageStream, cancelChatStream } from '@/api/chat-stream'
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
  startedAt: number | null
  /** Stream completion timestamp */
  completedAt: number | null
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

  /**
   * Set up the Tauri event listener for chat stream events
   */
  async function setupListener(): Promise<void> {
    if (unlistenFn) return

    unlistenFn = await listen<ChatStreamEvent>('chat:stream', (event) => {
      const data = event.payload
      const currentSessionId = sessionId()

      // Only process events for the current session
      if (data.session_id !== currentSessionId) return

      handleEvent(data)
    })
  }

  /**
   * Handle a chat stream event
   */
  function handleEvent(event: ChatStreamEvent): void {
    const kind = event.kind as ChatStreamKind

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
            })
          }
          break

        case 'tool_call_end':
          if ('tool_id' in kind) {
            const step = state.value.steps.find((s) => s.status === 'running')
            if (step && 'success' in kind) {
              step.status = kind.success ? 'completed' : 'failed'
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
          }
          break

        case 'failed':
          if ('error' in kind) {
            state.value.isStreaming = false
            state.value.error = kind.error
            if ('partial_content' in kind && kind.partial_content) {
              state.value.content = kind.partial_content
            }
          }
          break

        case 'cancelled':
          state.value.isStreaming = false
          if ('partial_content' in kind && kind.partial_content) {
            state.value.content = kind.partial_content
          }
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
    const end = state.value.completedAt || Date.now()
    return end - state.value.startedAt
  })
  const tokensPerSecond: ComputedRef<number> = computed(() => {
    const ms = duration.value
    if (ms === 0) return 0
    return (state.value.tokenCount / ms) * 1000
  })

  // Cleanup on unmount
  onUnmounted(() => {
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
