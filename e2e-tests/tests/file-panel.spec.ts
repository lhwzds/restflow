import { expect, test } from '@playwright/test'
import { cleanupTrackedState, createSessionForTest, goToWorkspace } from './helpers'

test.describe('File Panel', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('renders edit diffs in the inspector for file tool events', async ({ page }) => {
    const sessionId = await createSessionForTest(page)
    const runId = `run-file-${Date.now()}`
    const baseTime = Date.now()

    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON() as
        | {
            type?: string
            data?: {
              run_id?: string | null
              query?: {
                container?: { kind?: string | null; id?: string | null } | null
              } | null
            }
          }
        | undefined

      if (
        payload?.type === 'ListExecutionSessions' &&
        payload.data?.query?.container?.kind === 'workspace' &&
        payload.data?.query?.container?.id === sessionId
      ) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: [
              {
                id: runId,
                title: 'File edit run',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                root_run_id: runId,
                task_id: null,
                run_id: runId,
                parent_run_id: null,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime,
                ended_at: baseTime + 1,
                updated_at: baseTime + 1,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
            ],
          }),
        })
        return
      }

      if (payload?.type === 'GetExecutionRunThread' && payload.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: runId,
                title: 'File edit run',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                root_run_id: runId,
                task_id: null,
                run_id: runId,
                parent_run_id: null,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime,
                ended_at: baseTime + 1,
                updated_at: baseTime + 1,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
              timeline: {
                events: [
                  {
                    id: 'event-file-edit-1',
                    run_id: runId,
                    parent_run_id: null,
                    turn_id: 'turn-file-1',
                    actor_type: 'agent',
                    actor_name: 'Agent One',
                    category: 'tool_call',
                    timestamp: baseTime,
                    visibility: 'visible',
                    sequence: 0,
                    subflow_path: [],
                    message: null,
                    tool_call: {
                      tool_call_id: 'tool-file-edit-1',
                      tool_name: 'file',
                      phase: 'completed',
                      input_summary: 'Edit /tmp/test.txt',
                      output_ref: 'updated /tmp/test.txt',
                      error: null,
                      success: true,
                      duration_ms: 42,
                      input: {
                        action: 'edit',
                        path: '/tmp/test.txt',
                        old_string: 'const foo = 1\nconst keep = 2',
                        new_string: 'const bar = 1\nconst keep = 2\nconst added = 3',
                      },
                      output: {
                        action: 'edit',
                        path: '/tmp/test.txt',
                        written: true,
                      },
                    },
                    llm_call: null,
                    model_switch: null,
                    lifecycle: null,
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                ],
                stats: {
                  total_events: 1,
                  by_category: {},
                  time_range: null,
                  top_requested_models: [],
                  top_effective_models: [],
                  top_providers: [],
                  avg_llm_latency_ms: null,
                  avg_tool_latency_ms: null,
                },
              },
            },
          }),
        })
        return
      }

      await route.continue()
    })

    await page.goto(`/workspace/c/${sessionId}/r/${runId}`)
    await page.waitForLoadState('domcontentloaded')

    await page
      .getByTestId('run-group-run-group-turn-file-1')
      .locator('button')
      .first()
      .click()
    await page.getByTestId('run-group-child-view-event-file-edit-1').click()

    await expect(page.getByTestId('tool-panel')).toBeVisible()
    await expect(page.getByTestId('file-panel')).toBeVisible()
    await expect(page.getByTestId('file-diff-view')).toContainText('+2')
    await expect(page.getByTestId('file-diff-view')).toContainText('−1')
    await expect(page.getByTestId('file-diff-view')).toContainText('const foo = 1')
    await expect(page.getByTestId('file-diff-view')).toContainText('const bar = 1')
    await expect(page.getByTestId('file-diff-view')).toContainText('const added = 3')
  })
})
