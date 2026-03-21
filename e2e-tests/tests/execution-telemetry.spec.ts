import { expect, test } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type ChatSession = { id: string }
type ExecutionTimeline = {
  events: unknown[]
  stats: { total_events: number }
}
type ExecutionMetricsResponse = { samples: unknown[] }
type ProviderHealthResponse = { events: unknown[] }
type ExecutionLogResponse = { events: unknown[] }
type ExecutionTraceStats = { total_events: number }

test.describe('Execution Telemetry', () => {
  test('daemon exposes empty telemetry queries for a fresh session', async ({ page }) => {
    await goToWorkspace(page)

    const session = await requestIpc<ChatSession>(page, {
      type: 'CreateSession',
      data: {
        agent_id: null,
        model: 'gpt-5',
        name: 'Telemetry E2E Session',
        skill_id: null,
      },
    })

    const timeline = await requestIpc<ExecutionTimeline>(page, {
      type: 'GetExecutionTimeline',
      data: {
        query: {
          task_id: session.id,
          run_id: null,
          session_id: session.id,
          turn_id: null,
          agent_id: null,
          category: null,
          source: null,
          from_timestamp: null,
          to_timestamp: null,
          limit: 50,
          offset: 0,
        },
      },
    })
    expect(timeline.events).toEqual([])
    expect(timeline.stats.total_events).toBe(0)

    const metrics = await requestIpc<ExecutionMetricsResponse>(page, {
      type: 'GetExecutionMetrics',
      data: {
        query: {
          task_id: session.id,
          session_id: session.id,
          agent_id: null,
          metric_name: null,
          limit: 20,
        },
      },
    })
    expect(metrics.samples).toEqual([])

    const providerHealth = await requestIpc<ProviderHealthResponse>(page, {
      type: 'GetProviderHealth',
      data: {
        query: {
          provider: '__telemetry_e2e_missing_provider__',
          model: '__telemetry_e2e_missing_model__',
          limit: 20,
        },
      },
    })
    expect(providerHealth.events).toEqual([])

    const logs = await requestIpc<ExecutionLogResponse>(page, {
      type: 'QueryExecutionLogs',
      data: {
        query: {
          task_id: session.id,
          session_id: session.id,
          agent_id: null,
          level: null,
          limit: 20,
        },
      },
    })
    expect(logs.events).toEqual([])

    const stats = await requestIpc<ExecutionTraceStats>(page, {
      type: 'GetExecutionTraceStats',
      data: {
        task_id: session.id,
      },
    })
    expect(stats.total_events).toBe(0)
  })
})
