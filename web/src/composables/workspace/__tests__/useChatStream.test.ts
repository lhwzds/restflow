import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { listen } from '@tauri-apps/api/event'
import { useChatStream } from '../useChatStream'
import { sendChatMessageStream, cancelChatStream } from '@/api/chat-stream'
import { listToolTraces } from '@/api/tool-traces'
import type { ChatStreamEvent } from '@/types/generated/ChatStreamEvent'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@/api/chat-stream', () => ({
  sendChatMessageStream: vi.fn(),
  cancelChatStream: vi.fn(),
}))

vi.mock('@/api/tool-traces', () => ({
  listToolTraces: vi.fn(),
}))

describe('useChatStream', () => {
  let streamListener: ((event: { payload: ChatStreamEvent }) => void) | null = null
  const unlistenMock = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    streamListener = null
    vi.mocked(listToolTraces).mockResolvedValue([])
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
      timestamp: Date.now(),
      kind: { type: 'started', model: 'gpt-5' },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: Date.now(),
      kind: { type: 'ack', content: '收到，我开始处理。' },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: Date.now(),
      kind: { type: 'token', text: 'Hel', token_count: 1 },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-1',
      timestamp: Date.now(),
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
      timestamp: Date.now(),
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
      timestamp: Date.now(),
      kind: {
        type: 'completed',
        full_content: 'Hello world',
        duration_ms: 120,
        total_tokens: 12,
      },
    })
    await new Promise((r) => setTimeout(r, 0))

    expect(vm.stream.state.value.content).toBe('Hello world')
    expect(vm.stream.state.value.acknowledgement).toBe('收到，我开始处理。')
    expect(vm.stream.state.value.tokenCount).toBe(12)
    expect(vm.stream.state.value.steps).toHaveLength(1)
    expect(vm.stream.state.value.steps[0]?.status).toBe('failed')
    expect(listToolTraces).toHaveBeenCalledWith('session-1', 'msg-1', 200)
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
      timestamp: Date.now(),
      kind: { type: 'token', text: 'ignored', token_count: 1 },
    })

    expect(vm.stream.state.value.content).toBe('')

    await vm.stream.cancel()
    expect(cancelChatStream).toHaveBeenCalledWith('session-1', 'msg-2')

    wrapper.unmount()
  })

  it('replays persisted tool steps after completion', async () => {
    vi.mocked(sendChatMessageStream).mockResolvedValue('msg-3')
    vi.mocked(listToolTraces).mockResolvedValue([
      {
        id: 'evt-1',
        session_id: 'session-1',
        turn_id: 'msg-3',
        message_id: null,
        event_type: 'tool_call_started',
        tool_call_id: 'tool-42',
        tool_name: 'web_search',
        input: '{"query":"restflow"}',
        output: null,
        output_ref: null,
        success: null,
        duration_ms: null,
        error: null,
        created_at: 1,
      },
      {
        id: 'evt-2',
        session_id: 'session-1',
        turn_id: 'msg-3',
        message_id: null,
        event_type: 'tool_call_completed',
        tool_call_id: 'tool-42',
        tool_name: 'web_search',
        input: null,
        output: '{"items":1}',
        output_ref: null,
        success: true,
        duration_ms: 10,
        error: null,
        created_at: 2,
      },
    ])

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as {
      stream: ReturnType<typeof useChatStream>
    }

    await vm.stream.send('persisted')
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-3',
      timestamp: Date.now(),
      kind: {
        type: 'completed',
        full_content: 'done',
        duration_ms: 10,
        total_tokens: 2,
      },
    })
    await new Promise((r) => setTimeout(r, 0))

    expect(vm.stream.state.value.steps).toHaveLength(1)
    expect(vm.stream.state.value.steps[0]?.toolId).toBe('tool-42')
    expect(vm.stream.state.value.steps[0]?.status).toBe('completed')
    expect(vm.stream.state.value.steps[0]?.result).toBe('{"items":1}')

    wrapper.unmount()
  })

  it('formats subagent tool labels with useful context', async () => {
    vi.mocked(sendChatMessageStream).mockResolvedValue('msg-4')

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as {
      stream: ReturnType<typeof useChatStream>
    }

    await vm.stream.send('delegate')
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-4',
      timestamp: Date.now(),
      kind: {
        type: 'tool_call_start',
        tool_id: 'tool-spawn',
        tool_name: 'spawn_agent',
        arguments: '{"agent":"code-planner","task":"plan","model":"zai-coding-plan-glm-5"}',
      },
    })
    emitEvent({
      session_id: 'session-1',
      message_id: 'msg-4',
      timestamp: Date.now(),
      kind: {
        type: 'tool_call_end',
        tool_id: 'tool-spawn',
        result: '{"task_id":"1234567890abcdef","status":"spawned"}',
        success: true,
      },
    })

    expect(vm.stream.state.value.steps).toHaveLength(1)
    expect(vm.stream.state.value.steps[0]?.name).toBe('spawn_agent')
    expect(vm.stream.state.value.steps[0]?.displayName).toContain('code-planner')
    expect(vm.stream.state.value.steps[0]?.displayName).toContain('@zai-coding-plan-glm-5')
    expect(vm.stream.state.value.steps[0]?.displayName).toContain('#12345678')

    wrapper.unmount()
  })
})
