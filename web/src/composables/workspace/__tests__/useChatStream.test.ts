import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { listen } from '@tauri-apps/api/event'
import { useChatStream } from '../useChatStream'
import { sendChatMessageStream, cancelChatStream } from '@/api/chat-stream'
import type { ChatStreamEvent } from '@/types/generated/ChatStreamEvent'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@/api/chat-stream', () => ({
  sendChatMessageStream: vi.fn(),
  cancelChatStream: vi.fn(),
}))

describe('useChatStream', () => {
  let streamListener: ((event: { payload: ChatStreamEvent }) => void) | null = null
  const unlistenMock = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    streamListener = null
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      streamListener = handler as (event: { payload: ChatStreamEvent }) => void
      return unlistenMock
    })
  })

  function createHarness() {
    return mount(
      defineComponent({
        setup(_, { expose }) {
          const sessionId = ref<string | null>('session-1')
          const stream = useChatStream(() => sessionId.value)
          expose({ stream, sessionId })
          return () => h('div')
        },
      }),
    )
  }

  function emitEvent(event: ChatStreamEvent) {
    streamListener?.({ payload: event })
  }

  it('streams token and tool events into reactive state', async () => {
    vi.mocked(sendChatMessageStream).mockResolvedValue('msg-1')

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as {
      stream: ReturnType<typeof useChatStream>
    }

    await vm.stream.send('hello')

    expect(sendChatMessageStream).toHaveBeenCalledWith('session-1', 'hello')
    expect(vm.stream.state.value.messageId).toBe('msg-1')
    expect(vm.stream.isStreaming.value).toBe(true)

    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: BigInt(Date.now()),
      kind: { type: 'started', model: 'gpt-5' },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: BigInt(Date.now()),
      kind: { type: 'token', text: 'Hel', token_count: 1 },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: BigInt(Date.now()),
      kind: {
        type: 'tool_call_start',
        tool_id: 'tool-1',
        tool_name: 'web_search',
        arguments: '{"query":"hello"}',
      },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: BigInt(Date.now()),
      kind: {
        type: 'tool_call_end',
        tool_id: 'tool-1',
        result: '{"ok":false}',
        success: false,
      },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: BigInt(Date.now()),
      kind: {
        type: 'completed',
        full_content: 'Hello world',
        duration_ms: 120n,
        total_tokens: 12,
      },
    })

    expect(vm.stream.state.value.content).toBe('Hello world')
    expect(vm.stream.state.value.tokenCount).toBe(12)
    expect(vm.stream.state.value.steps).toHaveLength(1)
    expect(vm.stream.state.value.steps[0]?.status).toBe('failed')
    expect(vm.stream.isStreaming.value).toBe(false)

    wrapper.unmount()
    expect(unlistenMock).toHaveBeenCalled()
  })

  it('cleans up listener registered after unmount to prevent memory leak', async () => {
    let resolveListenPromise: ((unlisten: () => void) => void) | null = null
    const lateListen = vi.fn()

    // Make listen return a promise we control
    vi.mocked(listen).mockImplementationOnce(
      () =>
        new Promise<() => void>((resolve) => {
          resolveListenPromise = resolve
        }),
    )

    vi.mocked(sendChatMessageStream).mockResolvedValue('msg-leak')

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as {
      stream: ReturnType<typeof useChatStream>
    }

    // Start send which calls setupListener, but listen hasn't resolved yet
    const sendPromise = vm.stream.send('test')

    // Unmount while listen is still pending
    wrapper.unmount()

    // Now resolve the listen promise — the unlisten should be called immediately
    resolveListenPromise!(lateListen)
    await sendPromise.catch(() => {})
    // Wait a tick for microtasks
    await new Promise((r) => setTimeout(r, 0))

    expect(lateListen).toHaveBeenCalled()
  })

  it('ignores events from other sessions and supports cancellation', async () => {
    vi.mocked(sendChatMessageStream).mockResolvedValue('msg-2')
    vi.mocked(cancelChatStream).mockResolvedValue(undefined)

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as {
      stream: ReturnType<typeof useChatStream>
    }

    await vm.stream.send('cancel me')
    emitEvent({
      session_id: 'session-other',
      message_id: 'msg-2',
      timestamp: BigInt(Date.now()),
      kind: { type: 'token', text: 'ignored', token_count: 1 },
    })

    expect(vm.stream.state.value.content).toBe('')

    await vm.stream.cancel()
    expect(cancelChatStream).toHaveBeenCalledWith('session-1', 'msg-2')

    wrapper.unmount()
  })
})
