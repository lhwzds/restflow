import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import TrayDashboard from '../TrayDashboard.vue'
import type { TrayDashboardMetrics } from '@/composables/tray/useTrayDashboardMetrics'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'

const mockRefresh = vi.fn()

function createEmptyMetrics(): TrayDashboardMetrics {
  return {
    kpis: {
      totalAgents: 0,
      runningAgents: 0,
      activeAgents: 0,
      pausedAgents: 0,
      completedAgents: 0,
      failedAgents: 0,
      totalRuns: 0,
      successRate: null,
      totalTokens: 0,
      totalCostUsd: 0,
      avgDurationMs: null,
      lastRunAt: null,
    },
    trend: [],
    modelUsage: [],
    topAgents: [],
    lastEventAt: null,
  }
}

const dashboardState = {
  agents: ref<BackgroundAgent[]>([]),
  error: ref<string | null>(null),
  isLoading: ref(false),
  isRefreshing: ref(false),
  lastUpdatedAt: ref<number | null>(1_700_000_000_000),
  metrics: ref<TrayDashboardMetrics>(createEmptyMetrics()),
  refresh: mockRefresh,
}

vi.mock('@/composables/tray/useTrayDashboardMetrics', () => ({
  useTrayDashboardMetrics: () => dashboardState,
}))

vi.mock('@/api/tauri-client', () => ({
  isTauri: () => false,
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    error: vi.fn(),
  }),
}))

vi.mock('@tauri-apps/api/window', () => ({
  Window: {
    getByLabel: vi.fn().mockResolvedValue(null),
  },
  getCurrentWindow: vi.fn(() => ({
    label: 'tray-dashboard',
    hide: vi.fn().mockResolvedValue(undefined),
  })),
}))

function mountDashboard() {
  return mount(TrayDashboard, {
    global: {
      stubs: {
        Card: { template: '<section><slot /></section>' },
        CardHeader: { template: '<header><slot /></header>' },
        CardTitle: { template: '<h3><slot /></h3>' },
        CardContent: { template: '<div><slot /></div>' },
        Button: {
          template: '<button><slot /></button>',
        },
        AgentStatusBadge: {
          props: ['status'],
          template: '<span data-testid="agent-status">{{ status }}</span>',
        },
      },
    },
  })
}

describe('TrayDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockRefresh.mockResolvedValue(undefined)

    dashboardState.agents.value = [{ id: 'task-1', name: 'Agent One' } as BackgroundAgent]
    dashboardState.error.value = null
    dashboardState.isLoading.value = false
    dashboardState.isRefreshing.value = false
    dashboardState.lastUpdatedAt.value = 1_700_000_100_000
    dashboardState.metrics.value = {
      kpis: {
        totalAgents: 3,
        runningAgents: 2,
        activeAgents: 1,
        pausedAgents: 1,
        completedAgents: 1,
        failedAgents: 0,
        totalRuns: 10,
        successRate: 0.8,
        totalTokens: 12_500,
        totalCostUsd: 6.4,
        avgDurationMs: 820,
        lastRunAt: 1_700_000_000_000,
      },
      trend: [
        { startAt: 1_700_000_000_000, tokens: 500, costUsd: 0.2, durationMs: 800, runs: 1 },
        { startAt: 1_700_000_010_000, tokens: 400, costUsd: 0.1, durationMs: 600, runs: 1 },
      ],
      modelUsage: [
        { model: 'gpt-5', agentCount: 2, runningCount: 1, tokens: 9000, costUsd: 4.1 },
        { model: 'claude-sonnet-4-5', agentCount: 1, runningCount: 1, tokens: 3500, costUsd: 2.3 },
      ],
      topAgents: [
        {
          id: 'task-1',
          name: 'Agent One',
          status: 'running',
          updatedAt: 1_700_000_100_000,
          totalTokens: 5000,
          totalCostUsd: 2.5,
        },
      ],
      lastEventAt: 1_700_000_100_000,
    }
  })

  it('renders key KPI and list sections', async () => {
    const wrapper = mountDashboard()
    await flushPromises()

    expect(mockRefresh).toHaveBeenCalledTimes(1)
    expect(wrapper.get('[data-testid="tray-dashboard-root"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="tray-kpi-running"]').text()).toBe('2')
    expect(wrapper.get('[data-testid="tray-kpi-success-rate"]').text()).toContain('80.0%')
    expect(wrapper.get('[data-testid="tray-model-list"]').text()).toContain('gpt-5')
    expect(wrapper.get('[data-testid="tray-agent-list"]').text()).toContain('Agent One')
  })

  it('shows empty states when no background agents exist', async () => {
    dashboardState.agents.value = []
    dashboardState.metrics.value = {
      ...dashboardState.metrics.value,
      kpis: {
        ...dashboardState.metrics.value.kpis,
        totalAgents: 0,
        runningAgents: 0,
      },
      trend: [],
      modelUsage: [],
      topAgents: [],
    }

    const wrapper = mountDashboard()
    await flushPromises()

    expect(wrapper.text()).toContain('No background agents found.')
    expect(wrapper.text()).toContain('No model usage data yet.')
  })
})
