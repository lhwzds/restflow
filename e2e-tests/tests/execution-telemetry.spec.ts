import { expect, test } from '@playwright/test'
import { cleanupTrackedState, goToWorkspace, requestIpc } from './helpers'

type ExecutionTimeline = {
  events: unknown[]
  stats: { total_events: number }
}
type ExecutionMetricsResponse = { samples: unknown[] }
type ProviderHealthResponse = { events: unknown[] }
type ExecutionLogResponse = { events: unknown[] }
type ExecutionTraceStats = { total_events: number }

test.describe('Execution Telemetry', () => {
  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('run-scoped telemetry requests use canonical run-only IPC types', async ({ page }) => {
    await goToWorkspace(page)

    const runId = `run-${Date.now()}`

    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON()

      if (payload?.type === 'GetExecutionRunTimeline' && payload?.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              events: [{ id: 'timeline-event-1', run_id: runId }],
              stats: { total_events: 1 },
            },
          }),
        })
        return
      }

      if (payload?.type === 'GetExecutionRunMetrics' && payload?.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              samples: [{ id: 'metric-1', run_id: runId }],
            },
          }),
        })
        return
      }

      if (payload?.type === 'QueryExecutionRunLogs' && payload?.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              events: [{ id: 'log-1', run_id: runId }],
            },
          }),
        })
        return
      }

      await route.continue()
    })

    const timeline = await requestIpc<ExecutionTimeline>(page, {
      type: 'GetExecutionRunTimeline',
      data: { run_id: runId },
    })
    expect(timeline.events).toHaveLength(1)
    expect(timeline.stats.total_events).toBe(1)

    const metrics = await requestIpc<ExecutionMetricsResponse>(page, {
      type: 'GetExecutionRunMetrics',
      data: { run_id: runId },
    })
    expect(metrics.samples).toHaveLength(1)

    const logs = await requestIpc<ExecutionLogResponse>(page, {
      type: 'QueryExecutionRunLogs',
      data: { run_id: runId },
    })
    expect(logs.events).toHaveLength(1)
  })

  test('daemon exposes empty run-scoped telemetry for a missing run', async ({ page }) => {
    await goToWorkspace(page)
    const runId = `missing-run-${Date.now()}`

    const timeline = await requestIpc<ExecutionTimeline>(page, {
      type: 'GetExecutionRunTimeline',
      data: { run_id: runId },
    })
    expect(timeline.events).toEqual([])
    expect(timeline.stats.total_events).toBe(0)

    const metrics = await requestIpc<ExecutionMetricsResponse>(page, {
      type: 'GetExecutionRunMetrics',
      data: { run_id: runId },
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
      type: 'QueryExecutionRunLogs',
      data: { run_id: runId },
    })
    expect(logs.events).toEqual([])

    const stats = await requestIpc<ExecutionTraceStats>(page, {
      type: 'GetExecutionTraceStats',
      data: {
        run_id: null,
      },
    })
    expect(stats.total_events).toBe(0)
  })
})
