import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { useExecutionTelemetry } from '../useExecutionTelemetry'
import {
  getRunExecutionMetrics,
  getRunExecutionTimeline,
  queryRunExecutionLogs,
} from '@/api/execution-traces'

vi.mock('@/api/execution-traces', () => ({
  getRunExecutionTimeline: vi.fn(),
  getRunExecutionMetrics: vi.fn(),
  queryRunExecutionLogs: vi.fn(),
}))

function mountComposable(initialRunId: string | null = 'run-1') {
  const runId = ref(initialRunId)

  const wrapper = mount(
    defineComponent({
      setup(_, { expose }) {
        const state = useExecutionTelemetry(runId)
        expose({ runId, ...state })
        return () => null
      },
      template: '<div />',
    }),
  )

  return {
    runId,
    state: wrapper.vm as unknown as ReturnType<typeof useExecutionTelemetry> & {
      runId: typeof runId
    },
  }
}

describe('useExecutionTelemetry', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('queries timeline, metrics, and logs by run id', async () => {
    vi.mocked(getRunExecutionTimeline).mockResolvedValue({
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
    vi.mocked(getRunExecutionMetrics).mockResolvedValue({ samples: [] } as any)
    vi.mocked(queryRunExecutionLogs).mockResolvedValue({ events: [] } as any)

    mountComposable('run-1')
    await flushPromises()

    expect(getRunExecutionTimeline).toHaveBeenCalledWith('run-1')
    expect(getRunExecutionMetrics).toHaveBeenCalledWith('run-1')
    expect(queryRunExecutionLogs).toHaveBeenCalledWith('run-1')
  })

  it('keeps errors isolated between timeline, metrics, and logs', async () => {
    vi.mocked(getRunExecutionTimeline).mockRejectedValue(new Error('timeline failed'))
    vi.mocked(getRunExecutionMetrics).mockResolvedValue({ samples: [{ id: 'metric-1' }] } as any)
    vi.mocked(queryRunExecutionLogs).mockResolvedValue({ events: [{ id: 'log-1' }] } as any)

    const { state } = mountComposable('run-2')
    await flushPromises()

    expect(state.timelineError).toBe('timeline failed')
    expect(state.metricsError).toBeNull()
    expect(state.logsError).toBeNull()
    expect(state.timeline).toBeNull()
    expect((state.metrics as any)?.samples).toHaveLength(1)
    expect((state.logs as any)?.events).toHaveLength(1)
  })

  it('stays empty when no run id is selected', async () => {
    const { state } = mountComposable(null)
    await flushPromises()

    expect(getRunExecutionTimeline).not.toHaveBeenCalled()
    expect(getRunExecutionMetrics).not.toHaveBeenCalled()
    expect(queryRunExecutionLogs).not.toHaveBeenCalled()
    expect(state.timeline).toBeNull()
    expect(state.metrics).toBeNull()
    expect(state.logs).toBeNull()
  })
})
