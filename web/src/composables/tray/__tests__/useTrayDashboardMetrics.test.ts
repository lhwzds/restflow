import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import { buildTrayDashboardMetrics, useTrayDashboardMetrics } from '../useTrayDashboardMetrics'
import * as backgroundAgentApi from '@/api/background-agents'
import * as agentApi from '@/api/agents'

vi.mock('@/api/background-agents', () => ({
  listBackgroundAgents: vi.fn(),
  getBackgroundAgentEvents: vi.fn(),
}))

vi.mock('@/api/agents', () => ({
  listAgents: vi.fn(),
}))

function createBackgroundAgent(
  overrides: Partial<BackgroundAgent> & Pick<BackgroundAgent, 'id'>,
): BackgroundAgent {
  return {
    id: overrides.id,
    name: overrides.name ?? `Agent ${overrides.id}`,
    description: null,
    agent_id: overrides.agent_id ?? `agent-${overrides.id}`,
    chat_session_id: `session-${overrides.id}`,
    owns_chat_session: false,
    input: null,
    input_template: null,
    schedule: { type: 'manual' },
    execution_mode: 'api',
    timeout_secs: null,
    notification: { enabled: false },
    memory: { enabled: false },
    durability_mode: 'none',
    resource_limits: {},
    prerequisites: [],
    continuation: { enabled: false },
    continuation_total_iterations: 0,
    continuation_segments_completed: 0,
    status: overrides.status ?? 'active',
    created_at: overrides.created_at ?? 1_700_000_000_000,
    updated_at: overrides.updated_at ?? 1_700_000_100_000,
    last_run_at: overrides.last_run_at ?? null,
    next_run_at: null,
    success_count: overrides.success_count ?? 0,
    failure_count: overrides.failure_count ?? 0,
    total_tokens_used: overrides.total_tokens_used ?? 0,
    total_cost_usd: overrides.total_cost_usd ?? 0,
    last_error: null,
    webhook: null,
    summary_message_id: null,
  } as unknown as BackgroundAgent
}

function createTaskEvent(overrides: Partial<TaskEvent> & Pick<TaskEvent, 'id'>): TaskEvent {
  return {
    id: overrides.id,
    task_id: overrides.task_id ?? 'task-1',
    event_type: overrides.event_type ?? 'completed',
    timestamp: overrides.timestamp ?? 1_700_000_000_000,
    message: overrides.message ?? null,
    output: overrides.output ?? null,
    tokens_used: overrides.tokens_used ?? null,
    cost_usd: overrides.cost_usd ?? null,
    duration_ms: overrides.duration_ms ?? null,
    subflow_path: overrides.subflow_path ?? [],
  }
}

function createStoredAgent(id: string, model: string | null): StoredAgent {
  return {
    id,
    name: `Stored ${id}`,
    agent: { model } as StoredAgent['agent'],
    created_at: Date.now(),
    updated_at: Date.now(),
  }
}

describe('buildTrayDashboardMetrics', () => {
  it('aggregates KPI counts and model distribution', () => {
    const agents = [
      createBackgroundAgent({
        id: 'task-1',
        agent_id: 'agent-1',
        status: 'running',
        success_count: 5,
        failure_count: 1,
        total_tokens_used: 2000,
        total_cost_usd: 1.2,
      }),
      createBackgroundAgent({
        id: 'task-2',
        agent_id: 'agent-2',
        status: 'failed',
        success_count: 1,
        failure_count: 2,
        total_tokens_used: 900,
        total_cost_usd: 0.7,
      }),
    ]

    const eventsByTask = {
      'task-1': [
        createTaskEvent({ id: 'event-1', duration_ms: 2000, tokens_used: 100, cost_usd: 0.1 }),
      ],
      'task-2': [
        createTaskEvent({ id: 'event-2', duration_ms: 1000, tokens_used: 50, cost_usd: 0.05 }),
      ],
    }

    const result = buildTrayDashboardMetrics({
      agents,
      eventsByTask,
      modelByAgentId: new Map([
        ['agent-1', 'gpt-5'],
        ['agent-2', 'claude-sonnet-4-5'],
      ]),
      now: 1_700_000_100_000,
      bucketSizeMs: 60_000,
      bucketCount: 4,
    })

    expect(result.kpis.totalAgents).toBe(2)
    expect(result.kpis.runningAgents).toBe(1)
    expect(result.kpis.failedAgents).toBe(1)
    expect(result.kpis.totalRuns).toBe(9)
    expect(result.kpis.successRate).toBeCloseTo(6 / 9)
    expect(result.kpis.totalTokens).toBe(2900)
    expect(result.kpis.totalCostUsd).toBeCloseTo(1.9)
    expect(result.kpis.avgDurationMs).toBe(1500)

    expect(result.modelUsage).toHaveLength(2)
    expect(result.modelUsage[0]?.model).toBe('gpt-5')
  })

  it('maps events into trend buckets', () => {
    const result = buildTrayDashboardMetrics({
      agents: [],
      eventsByTask: {
        'task-1': [
          createTaskEvent({
            id: 'event-1',
            timestamp: 12_100,
            tokens_used: 50,
            duration_ms: 500,
          }),
          createTaskEvent({
            id: 'event-2',
            timestamp: 13_500,
            tokens_used: 30,
            duration_ms: 250,
          }),
        ],
      },
      modelByAgentId: new Map(),
      now: 14_000,
      bucketSizeMs: 1_000,
      bucketCount: 4,
    })

    const trendByStart = new Map(result.trend.map((bucket) => [bucket.startAt, bucket.tokens]))
    expect(result.trend).toHaveLength(4)
    expect(trendByStart.get(10_000)).toBe(0)
    expect(trendByStart.get(11_000)).toBe(0)
    expect(trendByStart.get(12_000)).toBe(50)
    expect(trendByStart.get(13_000)).toBe(30)
  })
})

describe('useTrayDashboardMetrics', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('refreshes from APIs and exposes computed metrics', async () => {
    vi.mocked(backgroundAgentApi.listBackgroundAgents).mockResolvedValue([
      createBackgroundAgent({
        id: 'task-1',
        agent_id: 'agent-1',
        status: 'running',
        success_count: 3,
        total_tokens_used: 1600,
        total_cost_usd: 0.4,
      }),
    ])
    vi.mocked(backgroundAgentApi.getBackgroundAgentEvents).mockResolvedValue([
      createTaskEvent({
        id: 'event-1',
        task_id: 'task-1',
        timestamp: 1_700_000_100_000,
        tokens_used: 100,
        cost_usd: 0.02,
        duration_ms: 850,
      }),
    ])
    vi.mocked(agentApi.listAgents).mockResolvedValue([createStoredAgent('agent-1', 'gpt-5')])

    const dashboard = useTrayDashboardMetrics()
    await dashboard.refresh()

    expect(backgroundAgentApi.listBackgroundAgents).toHaveBeenCalledTimes(1)
    expect(agentApi.listAgents).toHaveBeenCalledTimes(1)
    expect(backgroundAgentApi.getBackgroundAgentEvents).toHaveBeenCalledWith('task-1', 200)

    expect(dashboard.error.value).toBeNull()
    expect(dashboard.metrics.value.kpis.totalAgents).toBe(1)
    expect(dashboard.metrics.value.kpis.runningAgents).toBe(1)
    expect(dashboard.metrics.value.modelUsage[0]?.model).toBe('gpt-5')
  })
})
