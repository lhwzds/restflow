import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { useTaskStream } from '../useTaskStream'
import { streamClient } from '@/api/http-client'

const mockStore = {
  fetchTasks: vi.fn().mockResolvedValue(undefined),
}

vi.mock('@/api/http-client', () => ({
  streamClient: vi.fn(),
}))

vi.mock('@/stores/taskStore', () => ({
  useTaskStore: () => mockStore,
}))

type UseTaskStreamVm = {
  stream: ReturnType<typeof useTaskStream>
  taskId: { value: string | null }
}

async function* createFrames(frames: unknown[]): AsyncGenerator<unknown> {
  for (const frame of frames) {
    yield frame
  }
}

async function flushPromises(turns = 20): Promise<void> {
  for (let index = 0; index < turns; index += 1) {
    await Promise.resolve()
  }
}

function createHarness() {
  return mount(
    defineComponent({
      setup(_, { expose }) {
        const taskId = ref<string | null>('task-1')
        const stream = useTaskStream(() => taskId.value)
        expose({ stream, taskId })
        return () => h('div')
      },
    }),
  )
}

describe('useTaskStream', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.fetchTasks.mockClear()
  })

  it('subscribes with the canonical task stream request and updates state', async () => {
    vi.mocked(streamClient).mockReturnValue(
      createFrames([
        {
          stream_type: 'event',
          data: {
            event: {
              background_agent: {
                task_id: 'task-1',
                timestamp: 100,
                kind: { type: 'started' },
              },
            },
          },
        },
        {
          stream_type: 'event',
          data: {
            event: {
              background_agent: {
                task_id: 'task-1',
                timestamp: 125,
                kind: { type: 'completed', duration_ms: 25, result: 'done' },
              },
            },
          },
        },
      ]) as ReturnType<typeof streamClient>,
    )

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as UseTaskStreamVm

    await vm.stream.setupListeners()
    await flushPromises()

    expect(streamClient).toHaveBeenCalledWith(
      {
        type: 'SubscribeTaskEvents',
        data: { task_id: 'task-1' },
      },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    )
    expect(vm.stream.streamState.value.phase).toBe('Completed')
    expect(vm.stream.streamState.value.result).toBe('done')
    expect(mockStore.fetchTasks).toHaveBeenCalledTimes(2)

    wrapper.unmount()
  })

  it('records stream errors from the daemon transport', async () => {
    vi.mocked(streamClient).mockReturnValue(
      createFrames([
        {
          stream_type: 'error',
          data: {
            code: 500,
            kind: 'internal',
            message: 'boom',
            details: null,
          },
        },
      ]) as ReturnType<typeof streamClient>,
    )

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as UseTaskStreamVm

    await vm.stream.setupListeners()
    await flushPromises()

    expect(vm.stream.streamState.value.error).toBe('boom')
    expect(vm.stream.streamState.value.isStreaming).toBe(false)

    wrapper.unmount()
  })

  it('uses task-first error text when the stream transport throws a non-error value', async () => {
    vi.mocked(streamClient).mockReturnValue(
      (async function* () {
        throw 'transport exploded'
      })() as ReturnType<typeof streamClient>,
    )

    const wrapper = createHarness()
    const vm = wrapper.vm as unknown as UseTaskStreamVm

    await vm.stream.setupListeners()
    await flushPromises()

    expect(vm.stream.streamState.value.error).toBe('Task stream failed')
    expect(vm.stream.streamState.value.isStreaming).toBe(false)

    wrapper.unmount()
  })
})
