import { test, expect } from '@playwright/test'
import {
  cleanupTrackedState,
  createSessionForTest,
  goToWorkspace,
  requestIpc,
} from './helpers'

/**
 * Workspace Layout E2E Tests
 *
 * Tests the three-column chat-centric layout:
 * - Left sidebar: Session list with New Session button, agent filter, settings gear
 * - Center: Chat panel with message list and input area
 * - Right: Canvas panel (shown on demand)
 */
test.describe('Workspace Layout', () => {
  test.describe.configure({ mode: 'serial' })

  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('renders three-column layout', async ({ page }) => {
    // Left sidebar with session list
    await expect(page.getByTestId('session-list-new-session')).toBeVisible()

    // Center chat area with input
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Bottom bar with settings gear
    const settingsButton = page.getByRole('button', { name: 'Settings' })
    await expect(settingsButton).toBeVisible()
  })

  test('shows dark mode toggle in bottom bar', async ({ page }) => {
    // Dark mode toggle should be next to settings gear
    const bottomBar = page.locator('.border-t.border-border').last()
    const buttons = bottomBar.locator('button')
    // Settings gear + dark mode toggle = at least 2 buttons
    await expect(buttons).toHaveCount(2)
  })

  test('allows dragging the left sidebar to increase its width', async ({ page }) => {
    const sidebar = page.getByTestId('workspace-sidebar')
    const resizer = page.getByTestId('workspace-sidebar-resizer')
    await expect(sidebar).toBeVisible()
    await expect(resizer).toBeVisible()

    const before = await sidebar.boundingBox()
    if (!before) {
      throw new Error('Failed to read the initial sidebar width')
    }

    const handle = await resizer.boundingBox()
    if (!handle) {
      throw new Error('Failed to read the sidebar resizer bounds')
    }

    await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2)
    await page.mouse.down()
    await page.mouse.move(handle.x + 96, handle.y + handle.height / 2, { steps: 8 })
    await page.mouse.up()

    await expect
      .poll(async () => {
        const after = await sidebar.boundingBox()
        return after?.width ?? 0
      })
      .toBeGreaterThan(before.width)
  })

  test('New Session button is visible', async ({ page }) => {
    const newSessionBtn = page.getByTestId('session-list-new-session')
    await expect(newSessionBtn).toBeVisible()
  })

  test('agent filter dropdown is visible', async ({ page }) => {
    // Agent filter select with "All agents" placeholder
    const agentFilter = page.locator('button[role="combobox"]').first()
    await expect(agentFilter).toBeVisible()
  })

  test('chat input area is visible with agent and model selectors', async ({ page }) => {
    await createSessionForTest(page)

    // Textarea
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Send button
    await expect(page.getByTestId('chat-send-button')).toBeVisible()
  })

  test('send button is disabled when input is empty', async ({ page }) => {
    await createSessionForTest(page)
    const sendButton = page.getByTestId('chat-send-button')
    await expect(sendButton).toBeDisabled()
  })

  test('send button enables when text is entered', async ({ page }) => {
    await createSessionForTest(page)
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Hello')

    const sendButton = page.getByTestId('chat-send-button')
    await expect(sendButton).toBeEnabled()
  })

  test('shows empty state when no messages', async ({ page }) => {
    await createSessionForTest(page)

    // Empty state with placeholder text
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })

  test('shows a not-found state for unknown canonical containers', async ({ page }) => {
    await page.goto('/workspace/c/missing-container')
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId('workspace-container-not-found-state')).toBeVisible()
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toHaveCount(0)
  })

  test('keyboard hints are hidden in expanded chat mode', async ({ page }) => {
    await createSessionForTest(page)

    // In workspace layout, chat is always expanded (isExpanded=true),
    // so keyboard hints (Enter/Shift+Enter) are not shown
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await expect(textarea).toBeVisible()

    // Hints should NOT be visible in expanded mode
    await expect(page.locator('text=Shift+Enter')).not.toBeVisible()
  })

  test('keeps legacy sessions readable without rendering synthetic persisted tool steps', async ({ page }) => {
    const sessionId = await createSessionForTest(page)
    const userMessageId = `e2e-user-${Date.now()}`
    const assistantMessageId = `e2e-assistant-${Date.now()}`

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: userMessageId,
          role: 'user',
          content: 'Find the latest release notes',
          timestamp: Date.now(),
          execution: null,
        },
      },
    })

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: assistantMessageId,
          role: 'assistant',
          content: 'I found the release notes and summarized the changes.',
          timestamp: Date.now() + 1,
          execution: {
            steps: [
              {
                step_type: 'tool_call',
                name: 'web_search',
                status: 'completed',
                duration_ms: 1200,
              },
            ],
            duration_ms: 1500,
            tokens_used: 42,
            cost_usd: null,
            input_tokens: null,
            output_tokens: null,
            status: 'completed',
          },
        },
      },
    })

    await page.goto(`/workspace/c/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    const persistedStep = page.getByTestId(`persisted-step-${assistantMessageId}-0`)
    await expect(persistedStep).toHaveCount(0)
    await expect(page.getByTestId(`chat-message-${assistantMessageId}`)).toBeVisible()
  })

  test('keeps legacy non-tool execution summaries inside the message body without synthetic inline steps', async ({
    page,
  }) => {
    const sessionId = await createSessionForTest(page)
    const assistantMessageId = `e2e-assistant-llm-${Date.now()}`

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: assistantMessageId,
          role: 'assistant',
          content: 'Execution summary is available.',
          timestamp: Date.now(),
          execution: {
            steps: [
              {
                step_type: 'llm_call',
                name: 'gpt-5',
                status: 'completed',
                duration_ms: 420,
              },
              {
                step_type: 'model_switch',
                name: 'gpt-4 -> gpt-5',
                status: 'completed',
                duration_ms: null,
              },
            ],
            duration_ms: 600,
            tokens_used: 21,
            cost_usd: null,
            input_tokens: null,
            output_tokens: null,
            status: 'completed',
          },
        },
      },
    })

    await page.goto(`/workspace/c/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId(`persisted-step-${assistantMessageId}-0`)).toHaveCount(0)
    await expect(page.getByTestId(`persisted-step-${assistantMessageId}-1`)).toHaveCount(0)
    await expect(page.getByTestId(`chat-message-${assistantMessageId}`)).toBeVisible()
  })

  test('renders canonical session thread order while preserving full chat message content', async ({
    page,
  }) => {
    const sessionId = await createSessionForTest(page)
    const runId = 'run-1'
    const baseTime = Date.now()
    const userMessageId = `e2e-thread-user-${Date.now()}`
    const assistantMessageId = `e2e-thread-assistant-${Date.now()}`

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: userMessageId,
          role: 'user',
          content: 'Find the latest release notes',
          timestamp: baseTime,
          execution: null,
        },
      },
    })

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: assistantMessageId,
          role: 'assistant',
          content: 'I found the release notes and summarized the changes in detail.',
          timestamp: baseTime + 1,
          execution: null,
        },
      },
    })

    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON() as
        | { type?: string; data?: { query?: { run_id?: string | null }; run_id?: string | null } }
        | undefined

      if (
        payload?.type === 'GetExecutionRunThread' &&
        payload.data?.run_id === runId
      ) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: 'focus-1',
                title: 'Session focus',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                task_id: null,
                run_id: runId,
                parent_run_id: null,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime,
                ended_at: null,
                updated_at: baseTime + 2,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 3,
              },
              timeline: {
                events: [
                  {
                    id: 'event-user-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'message',
                    source: 'agent_executor',
                    timestamp: baseTime,
                    subflow_path: [],
                    run_id: runId,
                    parent_run_id: null,
                    session_id: sessionId,
                    turn_id: 'turn-1',
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: null,
                    model_switch: null,
                    lifecycle: null,
                    message: {
                      role: 'user',
                      content_preview: 'Find the latest release notes',
                      tool_call_count: null,
                    },
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                  {
                    id: 'event-tool-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'tool_call',
                    source: 'agent_executor',
                    timestamp: baseTime + 1,
                    subflow_path: [],
                    run_id: runId,
                    parent_run_id: null,
                    session_id: sessionId,
                    turn_id: 'turn-1',
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: {
                      tool_name: 'web_search',
                      phase: 'completed',
                      input_summary: 'release notes',
                      output_ref: null,
                      error: null,
                    },
                    model_switch: null,
                    lifecycle: null,
                    message: null,
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                  {
                    id: 'event-assistant-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'message',
                    source: 'agent_executor',
                    timestamp: baseTime + 2,
                    subflow_path: [],
                    run_id: runId,
                    parent_run_id: null,
                    session_id: sessionId,
                    turn_id: 'turn-1',
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: null,
                    model_switch: null,
                    lifecycle: null,
                    message: {
                      role: 'assistant',
                      content_preview: 'I found the release notes',
                      tool_call_count: 1,
                    },
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                ],
                stats: {
                  total_events: 3,
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

    const runGroup = page.getByTestId('run-group-run-group-turn-1')
    await expect(runGroup).toBeVisible()
    await expect(runGroup).toContainText('Turn')
    await expect(page.getByTestId('run-group-child-view-event-tool-1')).toHaveCount(0)
    await runGroup.locator('button').first().click()
    await expect(page.getByTestId('run-group-child-view-event-tool-1')).toBeVisible()
    await expect(page.getByTestId(`chat-message-${assistantMessageId}`)).toBeVisible()
    const toolRow = page.getByTestId('run-group-run-group-turn-1')
    const assistantRow = page.getByTestId(`chat-message-${assistantMessageId}`)
    await expect(
      assistantRow.getByText('I found the release notes and summarized the changes in detail.'),
    ).toBeVisible()
    const toolAppearsBeforeAssistant = await toolRow.evaluate(
      (toolNode, assistantTestId) => {
        const assistantNode = document.querySelector(`[data-testid="${assistantTestId}"]`)
        if (!assistantNode) return false
        return Boolean(toolNode.compareDocumentPosition(assistantNode) & Node.DOCUMENT_POSITION_FOLLOWING)
      },
      `chat-message-${assistantMessageId}`,
    )

    expect(toolAppearsBeforeAssistant).toBe(true)
  })

  test('keeps the center panel on the canonical run view after a streamed tool call completes', async ({
    page,
  }) => {
    const sessionId = await createSessionForTest(page)
    const turnId = 'turn-live-1'
    let streamRunId: string | null = null
    let persistedRunAttempts = 0

    await page.route('**/api/stream', async (route) => {
      const payload = route.request().postDataJSON() as
        | {
            type?: string
            data?: {
              session_id?: string | null
              stream_id?: string | null
            }
          }
        | undefined

      if (
        payload?.type === 'ExecuteChatSessionStream' &&
        payload.data?.session_id === sessionId &&
        payload.data.stream_id
      ) {
        streamRunId = payload.data.stream_id
        const frames = [
          { stream_type: 'start', data: { stream_id: streamRunId } },
          { stream_type: 'ack', data: { content: 'Running python...' } },
          {
            stream_type: 'tool_call',
            data: {
              id: 'tool-live-1',
              name: 'python3',
              arguments: { cmd: 'python3 /tmp/helloworld.py' },
            },
          },
          {
            stream_type: 'tool_result',
            data: {
              id: 'tool-live-1',
              result: 'Hello, World!',
              success: true,
            },
          },
          { stream_type: 'done', data: { total_tokens: 12 } },
        ]
          .map((frame) => JSON.stringify(frame))
          .join('\n')

        await route.fulfill({
          status: 200,
          contentType: 'application/x-ndjson',
          body: `${frames}\n`,
        })
        return
      }

      await route.continue()
    })

    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON() as
        | {
            type?: string
            data?: {
              run_id?: string | null
            }
          }
        | undefined

      if (
        payload?.type === 'GetExecutionRunThread' &&
        streamRunId &&
        payload.data?.run_id === streamRunId
      ) {
        if (persistedRunAttempts < 4) {
          persistedRunAttempts += 1
          await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
              response_type: 'Error',
              data: {
                code: 404,
                kind: 'not_found',
                message: 'Run not found yet',
                details: null,
              },
            }),
          })
          return
        }

        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: 'focus-live-1',
                title: 'Live run',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                task_id: null,
                run_id: streamRunId,
                parent_run_id: null,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: Date.now(),
                ended_at: Date.now() + 1,
                updated_at: Date.now() + 1,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 3,
              },
              timeline: {
                events: [
                  {
                    id: 'event-tool-live-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'tool_call',
                    source: 'agent_executor',
                    timestamp: Date.now(),
                    subflow_path: [],
                    run_id: streamRunId,
                    parent_run_id: null,
                    session_id: sessionId,
                    turn_id: turnId,
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: {
                      tool_call_id: 'tool-live-1',
                      tool_name: 'python3',
                      phase: 'completed',
                      input: '{"cmd":"python3 /tmp/helloworld.py"}',
                      input_summary: 'python3 /tmp/helloworld.py',
                      output: 'Hello, World!',
                      output_ref: null,
                      success: true,
                      error: null,
                      duration_ms: 50,
                    },
                    model_switch: null,
                    lifecycle: null,
                    message: null,
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                  {
                    id: 'event-lifecycle-live-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'lifecycle',
                    source: 'agent_executor',
                    timestamp: Date.now() + 1,
                    subflow_path: [],
                    run_id: streamRunId,
                    parent_run_id: null,
                    session_id: sessionId,
                    turn_id: turnId,
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: null,
                    model_switch: null,
                    lifecycle: {
                      status: 'run_completed',
                      message: null,
                      error: null,
                      ai_duration_ms: 50,
                    },
                    message: null,
                    metric_sample: null,
                    provider_health: null,
                    log_record: null,
                  },
                ],
                stats: {
                  total_events: 2,
                  by_category: {},
                  time_range: null,
                  top_requested_models: [],
                  top_effective_models: [],
                  top_providers: [],
                  avg_llm_latency_ms: null,
                  avg_tool_latency_ms: 50,
                },
              },
            },
          }),
        })
        return
      }

      await route.continue()
    })

    await page.locator('textarea[placeholder*="Ask the agent"]').fill('Run hello world')
    await page.getByTestId('chat-send-button').click()

    await expect
      .poll(() => page.url())
      .toContain(`/workspace/c/${sessionId}/r/`)

    await expect
      .poll(() => streamRunId)
      .not.toBeNull()

    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${streamRunId}$`))
    await expect(page.getByTestId(`run-group-run-group-${turnId}`)).toBeVisible()
    await expect(page.getByTestId(`run-group-run-group-${turnId}`)).toContainText('Turn')
    await expect(page.getByTestId(`run-group-run-group-${turnId}`)).toContainText('python3')
    await expect(page.locator('text=minimax:tool_call')).toHaveCount(0)
    expect(persistedRunAttempts).toBe(4)
  })

  test('lazily expands child and grandchild runs on the canonical run tree', async ({ page }) => {
    const sessionId = await createSessionForTest(page)
    const parentRunId = `run-parent-${Date.now()}`
    const childRunId = `run-child-${Date.now()}`
    const grandchildRunId = `run-grandchild-${Date.now()}`
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
                id: parentRunId,
                title: 'Parent run',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: parentRunId,
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

      if (
        payload?.type === 'ListChildExecutionSessions' &&
        payload.data?.query?.parent_run_id === parentRunId
      ) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: [
              {
                id: childRunId,
                title: 'Child run',
                subtitle: null,
                status: 'completed',
                kind: 'subagent_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: childRunId,
                parent_run_id: parentRunId,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime + 2,
                ended_at: baseTime + 3,
                updated_at: baseTime + 3,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
            ],
          }),
        })
        return
      }

      if (
        payload?.type === 'ListChildExecutionSessions' &&
        payload.data?.query?.parent_run_id === childRunId
      ) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: [
              {
                id: grandchildRunId,
                title: 'Grandchild run',
                subtitle: null,
                status: 'completed',
                kind: 'subagent_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: grandchildRunId,
                parent_run_id: childRunId,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime + 4,
                ended_at: baseTime + 5,
                updated_at: baseTime + 5,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
            ],
          }),
        })
        return
      }

      if (
        payload?.type === 'ListChildExecutionSessions' &&
        payload.data?.query?.parent_run_id === grandchildRunId
      ) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: [],
          }),
        })
        return
      }

      if (payload?.type === 'GetExecutionRunThread' && payload.data?.run_id === parentRunId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: parentRunId,
                title: 'Parent run',
                subtitle: null,
                status: 'completed',
                kind: 'workspace_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: parentRunId,
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
                events: [],
                stats: {},
              },
            },
          }),
        })
        return
      }

      if (payload?.type === 'GetExecutionRunThread' && payload.data?.run_id === childRunId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: childRunId,
                title: 'Child run',
                subtitle: null,
                status: 'completed',
                kind: 'subagent_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: childRunId,
                parent_run_id: parentRunId,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime + 2,
                ended_at: baseTime + 3,
                updated_at: baseTime + 3,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
              timeline: {
                events: [
                  {
                    id: 'event-child-tool-1',
                    run_id: childRunId,
                    parent_run_id: parentRunId,
                    turn_id: 'turn-child-1',
                    actor_type: 'agent',
                    actor_name: 'Agent One',
                    category: 'tool_call',
                    timestamp: baseTime + 2,
                    visibility: 'visible',
                    sequence: 0,
                    subflow_path: ['child'],
                    message: null,
                    tool_call: {
                      tool_call_id: 'tool-child-1',
                      tool_name: 'http_request',
                      phase: 'completed',
                      input_summary: 'GET https://example.com',
                      output_ref: 'status 200',
                      error: null,
                      success: true,
                      duration_ms: 25,
                      input: {
                        method: 'GET',
                        url: 'https://example.com',
                      },
                      output: {
                        status: 200,
                        body: {
                          ok: true,
                        },
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

      if (payload?.type === 'GetExecutionRunThread' && payload.data?.run_id === grandchildRunId) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            response_type: 'Success',
            data: {
              focus: {
                id: grandchildRunId,
                title: 'Grandchild run',
                subtitle: null,
                status: 'completed',
                kind: 'subagent_run',
                container_id: sessionId,
                root_run_id: parentRunId,
                task_id: null,
                run_id: grandchildRunId,
                parent_run_id: childRunId,
                session_id: sessionId,
                agent_id: 'agent-1',
                effective_model: 'gpt-5',
                provider: null,
                started_at: baseTime + 4,
                ended_at: baseTime + 5,
                updated_at: baseTime + 5,
                source_channel: 'workspace',
                source_conversation_id: null,
                event_count: 1,
              },
              timeline: {
                events: [
                  {
                    id: 'event-grandchild-message-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'message',
                    source: 'agent_executor',
                    timestamp: baseTime + 4,
                    subflow_path: ['child', 'grandchild'],
                    run_id: grandchildRunId,
                    parent_run_id: childRunId,
                    session_id: sessionId,
                    turn_id: 'turn-grandchild-1',
                    requested_model: 'gpt-5',
                    effective_model: 'gpt-5',
                    provider: 'openai',
                    attempt: 1,
                    llm_call: null,
                    tool_call: null,
                    model_switch: null,
                    lifecycle: null,
                    message: {
                      role: 'assistant',
                      content_preview: 'Grandchild output',
                      tool_call_count: null,
                    },
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

    await page.goto(`/workspace/c/${sessionId}/r/${parentRunId}`)
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId('tool-panel')).toBeVisible()
    await expect(page.getByTestId('run-overview-title')).toContainText('Parent run')
    const childRunOverviewButton = page.getByTestId(`run-overview-child-run-${childRunId}`)
    const childRunRow = page.getByTestId(`workspace-run-${sessionId}-${childRunId}`)
    const grandchildRunRow = page.getByTestId(`workspace-run-${sessionId}-${grandchildRunId}`)
    const parentToggle = page.getByTestId(`workspace-run-toggle-${sessionId}-${parentRunId}`)
    await expect(page.getByTestId('run-overview-child-run-list')).toBeVisible()
    await expect(childRunOverviewButton).toBeVisible()
    await expect(childRunRow).toHaveCount(0)
    await expect(parentToggle).toBeVisible()
    await expect(page.getByTestId(`thread-item-view-child-run-${childRunId}`)).toHaveCount(0)
    await parentToggle.click()
    await expect(childRunRow).toBeVisible()
    await expect(childRunRow).toHaveAttribute('data-run-depth', '1')
    await expect(childRunRow).toContainText('Child')
    const childToggle = page.getByTestId(`workspace-run-toggle-${sessionId}-${childRunId}`)
    await expect(grandchildRunRow).toHaveCount(0)
    await childToggle.click()
    await expect(grandchildRunRow).toBeVisible()
    await expect(grandchildRunRow).toHaveAttribute('data-run-depth', '2')
    await grandchildRunRow.click()

    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${grandchildRunId}$`))
    await expect(page.getByTestId('run-overview-title')).toContainText('Grandchild run')
    await expect(page.getByTestId('run-breadcrumb')).toBeVisible()
    await expect(page.getByTestId('run-breadcrumb')).toContainText('Grandchild run')
    await expect(page.getByTestId('run-breadcrumb-node-root')).toContainText('Parent run')
    await expect(page.getByTestId('run-breadcrumb-node-parent')).toContainText('Child run')
    await page.getByTestId('chat-message-event-grandchild-message-1').click()
    await expect(page.getByTestId('tool-panel-run-navigation')).toBeVisible()
    await expect(page.getByTestId('tool-panel-run-nav-root')).toContainText('Parent run')
    await expect(page.getByTestId('tool-panel-run-nav-parent')).toContainText('Child run')
    await page.getByTestId('tool-panel-run-nav-parent').click()
    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${childRunId}$`))
    await expect(page.getByTestId('run-overview-title')).toContainText('Child run')
    await page.getByTestId('tool-panel-run-nav-root').click()
    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${parentRunId}$`))
    await expect(page.getByTestId('run-overview-title')).toContainText('Parent run')
    await childRunOverviewButton.click()
    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${childRunId}$`))
    await page.getByTestId('run-breadcrumb-node-root').click()
    await expect(page).toHaveURL(new RegExp(`/workspace/c/${sessionId}/r/${parentRunId}$`))
  })
})

test.describe('Session List', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('shows session list state', async ({ page }) => {
    await expect(page.getByTestId('session-list-new-session')).toBeVisible()
    await expect
      .poll(async () => {
        const workspaceCount = await page.locator('[data-testid^="workspace-folder-"]').count()
        const backgroundCount = await page.locator('[data-testid^="background-folder-"]').count()
        const externalCount = await page.locator('[data-testid^="external-folder-"]').count()
        const emptyCount = await page.getByTestId('session-empty-state').count()
        return workspaceCount + backgroundCount + externalCount + emptyCount > 0
      })
      .toBe(true)

    if ((await page.locator('[data-testid^="workspace-folder-"]').count()) > 0) {
      await expect(page.locator('[data-testid^="workspace-folder-"]').first()).toBeVisible()
      return
    }
    if ((await page.locator('[data-testid^="background-folder-"]').count()) > 0) {
      await expect(page.locator('[data-testid^="background-folder-"]').first()).toBeVisible()
      return
    }
    if ((await page.locator('[data-testid^="external-folder-"]').count()) > 0) {
      await expect(page.locator('[data-testid^="external-folder-"]').first()).toBeVisible()
      return
    }
    await expect(page.getByTestId('session-empty-state')).toBeVisible()
  })

  test('New Session button clears current session', async ({ page }) => {
    await createSessionForTest(page)
    await createSessionForTest(page)

    // Should show empty conversation state
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })
})
