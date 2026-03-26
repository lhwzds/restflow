import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { useChatStream } from '../useChatStream'
import { cancelChatStream, openChatStream } from '@/api/chat-stream'
import { queryExecutionTraces } from '@/api/execution-traces'
import type { StreamFrame } from '@/types/generated/StreamFrame'

vi.mock('@/api/chat-stream', () => ({
  cancelChatStream: vi.fn(),
  openChatStream: vi.fn(),
}))

vi.mock('@/api/execution-traces', () => ({
  queryExecutionTraces: vi.fn(),
}))

async function* createFrames(frames: StreamFrame[]): AsyncGenerator<StreamFrame> {
  for (const frame of frames) {
    yield frame
  }
}

async function flushPromises(turns = 20): Promise<void> {
  for (let index = 0; index < turns; index += 1) {
    await Promise.resolve()
  }
}

describe('useChatStream', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(queryExecutionTraces).mockResolvedValue([])
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

  it('streams ack, text, and tool events into reactive state', async () => {
    vi.mocked(openChatStream).mockReturnValue({
      streamId: 'msg-1',
      frames: createFrames([
        { stream_type: 'start', data: { stream_id: 'msg-1' } },
        { stream_type: 'ack', data: { content: 'Working on it' } },
        { stream_type: 'data', data: { content: 'Hello' } },
        {
          stream_type: 'tool_call',
          data: { id: 'tool-1', name: 'web_search', arguments: { query: 'hello' } },
        },
        {
          stream_type: 'tool_result',
          data: { id: 'tool-1', result: '{"ok":true}', success: true },
        },
        { stream_type: 'done', data: { total_tokens: 12 } },
      ]),
    })

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as { stream: ReturnType<typeof useChatStream> }

    const messageId = await vm.stream.send('hello')
    await flushPromises()

    expect(messageId).toBe('msg-1')
    expect(openChatStream).toHaveBeenCalledWith('session-1', 'hello', expect.any(AbortSignal))
    expect(vm.stream.state.value.messageId).toBe('msg-1')
    expect(vm.stream.state.value.acknowledgement).toBe('Working on it')
    expect(vm.stream.state.value.content).toBe('Working on it\n\nHello')
    expect(vm.stream.state.value.steps).toHaveLength(1)
    expect(vm.stream.state.value.steps[0]?.status).toBe('completed')
    expect(vm.stream.isStreaming.value).toBe(false)
    expect(queryExecutionTraces).toHaveBeenCalledWith({
      task_id: null,
      run_id: null,
      parent_run_id: null,
      session_id: 'session-1',
      turn_id: 'msg-1',
      agent_id: null,
      category: null,
      source: null,
      from_timestamp: null,
      to_timestamp: null,
      limit: 200,
      offset: 0,
    })

    wrapper.unmount()
  })

  it('syncs persisted events by session_id only so background-linked sessions can resolve their trace', async () => {
    vi.mocked(openChatStream).mockReturnValue({
      streamId: 'msg-4',
      frames: createFrames([{ stream_type: 'done', data: { total_tokens: 1 } }]),
    })

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as { stream: ReturnType<typeof useChatStream> }

    await vm.stream.send('hello')
    await flushPromises()

    expect(queryExecutionTraces).toHaveBeenCalledWith(
      expect.objectContaining({
        task_id: null,
        session_id: 'session-1',
        turn_id: 'msg-4',
      }),
    )

    wrapper.unmount()
  })

  it('cancels the active stream through the shared contract', async () => {
    vi.mocked(openChatStream).mockReturnValue({
      streamId: 'msg-2',
      frames: createFrames([{ stream_type: 'start', data: { stream_id: 'msg-2' } }]),
    })
    vi.mocked(cancelChatStream).mockResolvedValue(undefined)

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as { stream: ReturnType<typeof useChatStream> }

    await vm.stream.send('cancel me')
    await vm.stream.cancel()

    expect(cancelChatStream).toHaveBeenCalledWith('msg-2')
    expect(vm.stream.isStreaming.value).toBe(false)

    wrapper.unmount()
  })

  it('records stream errors and fails running steps', async () => {
    vi.mocked(openChatStream).mockReturnValue({
      streamId: 'msg-3',
      frames: createFrames([
        {
          stream_type: 'tool_call',
          data: { id: 'tool-2', name: 'web_search', arguments: { query: 'hello' } },
        },
        {
          stream_type: 'error',
          data: { code: 500, kind: 'internal', message: 'boom', details: null },
        },
      ]),
    })

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as { stream: ReturnType<typeof useChatStream> }

    await vm.stream.send('hello')
    await flushPromises()

    expect(vm.stream.state.value.error).toBe('boom')
    expect(vm.stream.state.value.steps[0]?.status).toBe('failed')
    expect(vm.stream.isStreaming.value).toBe(false)

    wrapper.unmount()
  })

  it('rejects duplicate sends while a stream is active', async () => {
    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as { stream: ReturnType<typeof useChatStream> }
    vm.stream.state.value.isStreaming = true

    await expect(vm.stream.send('duplicate')).rejects.toThrow(
      'Streaming response is already in progress',
    )

    wrapper.unmount()
  })
})
