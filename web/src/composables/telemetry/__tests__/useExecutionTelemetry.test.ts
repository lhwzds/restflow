import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { useExecutionTelemetry } from '../useExecutionTelemetry'
import {
  getExecutionMetrics,
  getExecutionTimeline,
  queryExecutionLogs,
} from '@/api/execution-traces'

vi.mock('@/api/execution-traces', () => ({
  getExecutionTimeline: vi.fn(),
  getExecutionMetrics: vi.fn(),
  queryExecutionLogs: vi.fn(),
}))

function mountComposable(initialTaskId = 'task-1') {
  const taskId = ref(initialTaskId)

  const wrapper = mount(
    defineComponent({
      setup(_, { expose }) {
        const state = useExecutionTelemetry(taskId)
        expose({ taskId, ...state })
        return () => null
      },
      template: '<div />',
    }),
  )

  return {
    taskId,
    state: wrapper.vm as unknown as ReturnType<typeof useExecutionTelemetry> & {
      taskId: typeof taskId
    },
  }
}

describe('useExecutionTelemetry', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('queries timeline, metrics, and logs by task id', async () => {
    vi.mocked(getExecutionTimeline).mockResolvedValue({
      events: [],
      stats: {
        total_events: 0n,
        llm_call_count: 0n,
        tool_call_count: 0n,
        model_switch_count: 0n,
        lifecycle_count: 0n,
        message_count: 0n,
        metric_sample_count: 0n,
        provider_health_count: 0n,
        log_record_count: 0n,
        total_tokens: 0n,
        total_cost_usd: 0,
        time_range: null,
      },
    } as any)
    vi.mocked(getExecutionMetrics).mockResolvedValue({ samples: [] } as any)
    vi.mocked(queryExecutionLogs).mockResolvedValue({ events: [] } as any)

    mountComposable('task-1')
    await flushPromises()

    expect(getExecutionTimeline).toHaveBeenCalledWith(
      expect.objectContaining({
        task_id: 'task-1',
        limit: 200,
      }),
    )
    expect(getExecutionMetrics).toHaveBeenCalledWith(
      expect.objectContaining({
        task_id: 'task-1',
        limit: 100,
      }),
    )
    expect(queryExecutionLogs).toHaveBeenCalledWith(
      expect.objectContaining({
        task_id: 'task-1',
        limit: 100,
      }),
    )
  })

  it('keeps errors isolated between timeline, metrics, and logs', async () => {
    vi.mocked(getExecutionTimeline).mockRejectedValue(new Error('timeline failed'))
    vi.mocked(getExecutionMetrics).mockResolvedValue({ samples: [{ id: 'metric-1' }] } as any)
    vi.mocked(queryExecutionLogs).mockResolvedValue({ events: [{ id: 'log-1' }] } as any)

    const { state } = mountComposable('task-2')
    await flushPromises()

    expect(state.timelineError).toBe('timeline failed')
    expect(state.metricsError).toBeNull()
    expect(state.logsError).toBeNull()
    expect(state.timeline).toBeNull()
    expect((state.metrics as any)?.samples).toHaveLength(1)
    expect((state.logs as any)?.events).toHaveLength(1)
  })
})
