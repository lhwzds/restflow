import { describe, it, expect, vi, beforeEach } from 'vitest'
import { computed, defineComponent, ref } from 'vue'
import { mount } from '@vue/test-utils'
import ExecutionTelemetryViewer from '../ExecutionTelemetryViewer.vue'
import { useExecutionTelemetry } from '@/composables/telemetry/useExecutionTelemetry'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params?.count !== undefined ? `${key}:${params.count}` : key,
  }),
}))

vi.mock('@/composables/telemetry/useExecutionTelemetry', () => ({
  useExecutionTelemetry: vi.fn(),
}))

vi.mock('@/components/ui/tabs', () => ({
  Tabs: defineComponent({
    template: '<div><slot /></div>',
  }),
  TabsList: defineComponent({
    template: '<div><slot /></div>',
  }),
  TabsTrigger: defineComponent({
    template: '<button><slot /></button>',
  }),
  TabsContent: defineComponent({
    template: '<div><slot /></div>',
  }),
}))

function buildTelemetryState(overrides?: Partial<ReturnType<typeof useExecutionTelemetry>>) {
  return {
    timeline: ref(overrides?.timeline?.value ?? null),
    metrics: ref(overrides?.metrics?.value ?? null),
    logs: ref(overrides?.logs?.value ?? null),
    isLoadingTimeline: ref(false),
    isLoadingMetrics: ref(false),
    isLoadingLogs: ref(false),
    timelineError: ref(null),
    metricsError: ref(null),
    logsError: ref(null),
    refresh: vi.fn(),
  }
}

describe('ExecutionTelemetryViewer', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('renders an empty timeline state', () => {
    vi.mocked(useExecutionTelemetry).mockReturnValue(
      buildTelemetryState({
        timeline: computed(() => ({
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
        }) as any),
        metrics: computed(() => ({ samples: [] }) as any),
        logs: computed(() => ({ events: [] }) as any),
      }) as any,
    )

    const wrapper = mount(ExecutionTelemetryViewer, {
      props: { taskId: 'task-1' },
    })

    expect(wrapper.get('[data-testid="execution-telemetry-empty"]').text()).toContain(
      'backgroundAgent.timelineEmpty',
    )
  })

  it('renders timeline, metrics, and logs data', () => {
    vi.mocked(useExecutionTelemetry).mockReturnValue(
      buildTelemetryState({
        timeline: computed(() => ({
          events: [
            {
              id: 'event-1',
              timestamp: 1710000000000,
              category: 'model_switch',
              attempt: 2,
              effective_model: 'minimax-coding-plan-m2-5',
              model_switch: {
                from_model: 'minimax-coding-plan-m2-5-highspeed',
                to_model: 'minimax-coding-plan-m2-5',
                reason: 'failover',
                success: true,
              },
            },
          ],
          stats: {
            total_events: 1n,
            llm_call_count: 0n,
            tool_call_count: 0n,
            model_switch_count: 1n,
            lifecycle_count: 0n,
            message_count: 0n,
            metric_sample_count: 0n,
            provider_health_count: 0n,
            log_record_count: 0n,
            total_tokens: 0n,
            total_cost_usd: 0,
            time_range: null,
          },
        }) as any),
        metrics: computed(() => ({
          samples: [
            {
              id: 'metric-1',
              timestamp: 1710000000001,
              category: 'metric_sample',
              metric_sample: {
                name: 'llm_total_tokens',
                value: 42,
                unit: 'tokens',
                dimensions: [],
              },
            },
          ],
        }) as any),
        logs: computed(() => ({
          events: [
            {
              id: 'log-1',
              timestamp: 1710000000002,
              category: 'log_record',
              log_record: {
                level: 'warn',
                message: 'Model failover happened',
                fields: [],
              },
            },
          ],
        }) as any),
      }) as any,
    )

    const wrapper = mount(ExecutionTelemetryViewer, {
      props: { taskId: 'task-1' },
    })

    expect(wrapper.text()).toContain('minimax-coding-plan-m2-5-highspeed → minimax-coding-plan-m2-5')
    expect(wrapper.text()).toContain('llm_total_tokens')
    expect(wrapper.text()).toContain('Model failover happened')
    expect(wrapper.findAll('[data-testid="execution-telemetry-event"]')).toHaveLength(1)
    expect(wrapper.findAll('[data-testid="execution-telemetry-metric"]')).toHaveLength(1)
    expect(wrapper.findAll('[data-testid="execution-telemetry-log"]')).toHaveLength(1)
  })
})
