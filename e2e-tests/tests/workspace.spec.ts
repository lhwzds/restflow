import { test, expect } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type SessionSummary = {
  id: string
}

/**
 * Workspace Layout E2E Tests
 *
 * Tests the three-column chat-centric layout:
 * - Left sidebar: Session list with New Session button, agent filter, settings gear
 * - Center: Chat panel with message list and input area
 * - Right: Canvas panel (shown on demand)
 */
test.describe('Workspace Layout', () => {
  async function openFreshWorkspaceSession(page: import('@playwright/test').Page) {
    const beforeSessions = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
    const beforeIds = new Set(beforeSessions.map((session) => session.id))

    await page.getByRole('button', { name: 'New Session' }).click()

    await expect
      .poll(async () => {
        const sessions = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
        const created = sessions.find((session) => !beforeIds.has(session.id))
        return created?.id ?? null
      })
      .not.toBeNull()

    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible({
      timeout: 15000,
    })

    const afterSessions = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
    const created = afterSessions.find((session) => !beforeIds.has(session.id))
    if (!created) {
      throw new Error('Failed to locate the newly created workspace session')
    }

    return created.id
  }

  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
    await openFreshWorkspaceSession(page)
  })

  test('renders three-column layout', async ({ page }) => {
    // Left sidebar with session list
    await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible()

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

  test('New Session button is visible', async ({ page }) => {
    const newSessionBtn = page.getByRole('button', { name: 'New Session' })
    await expect(newSessionBtn).toBeVisible()
  })

  test('agent filter dropdown is visible', async ({ page }) => {
    // Agent filter select with "All agents" placeholder
    const agentFilter = page.locator('button[role="combobox"]').first()
    await expect(agentFilter).toBeVisible()
  })

  test('chat input area is visible with agent and model selectors', async ({ page }) => {
    // Textarea
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Send button
    await expect(page.getByTestId('chat-send-button')).toBeVisible()
  })

  test('send button is disabled when input is empty', async ({ page }) => {
    const sendButton = page.getByTestId('chat-send-button')
    await expect(sendButton).toBeDisabled()
  })

  test('send button enables when text is entered', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Hello')

    const sendButton = page.getByTestId('chat-send-button')
    await expect(sendButton).toBeEnabled()
  })

  test('shows empty state when no messages', async ({ page }) => {
    await page.getByRole('button', { name: 'New Session' }).click()

    // Empty state with placeholder text
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })

  test('keyboard hints are hidden in expanded chat mode', async ({ page }) => {
    // In workspace layout, chat is always expanded (isExpanded=true),
    // so keyboard hints (Enter/Shift+Enter) are not shown
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await expect(textarea).toBeVisible()

    // Hints should NOT be visible in expanded mode
    await expect(page.locator('text=Shift+Enter')).not.toBeVisible()
  })

  test('shows persisted tool steps inline in chat and opens the detail panel', async ({ page }) => {
    const sessionId = await openFreshWorkspaceSession(page)
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

    await page.goto(`/workspace/sessions/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    const persistedStep = page.getByTestId(`persisted-step-${assistantMessageId}-0`)
    await expect(persistedStep).toBeVisible()
    await expect(page.getByTestId(`chat-message-${assistantMessageId}`)).toBeVisible()

    await page.getByTestId(`persisted-step-view-${assistantMessageId}-0`).click()

    await expect(page.getByTestId('generic-json-panel')).toBeVisible()
    await expect(page.locator('text=Detailed persisted tool payload is not available yet.')).toBeVisible()
  })

  test('shows persisted non-tool execution steps inline and opens generic detail view', async ({
    page,
  }) => {
    const sessionId = await openFreshWorkspaceSession(page)
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

    await page.goto(`/workspace/sessions/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId(`persisted-step-${assistantMessageId}-0`)).toBeVisible()
    await expect(page.getByTestId(`persisted-step-${assistantMessageId}-1`)).toBeVisible()

    await page.getByTestId(`persisted-step-view-${assistantMessageId}-1`).click()

    await expect(page.getByTestId('generic-json-panel')).toBeVisible()
    await expect(page.locator('text=Persisted execution step summary.')).toBeVisible()
    await expect(page.getByText('model_switch: gpt-4 -> gpt-5')).toBeVisible()
  })

  test('renders canonical session thread order while preserving full chat message content', async ({
    page,
  }) => {
    const sessionId = await openFreshWorkspaceSession(page)
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
          content: 'I found the release notes and summarized the changes in detail.',
          timestamp: Date.now() + 1,
          execution: null,
        },
      },
    })

    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON() as
        | { type?: string; data?: { query?: { session_id?: string | null } } }
        | undefined

      if (
        payload?.type === 'GetExecutionThread' &&
        payload.data?.query?.session_id === sessionId
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
                source_kind: 'workspace_session',
                container_id: sessionId,
                task_id: null,
                run_id: null,
                parent_run_id: null,
                session_id: sessionId,
                agent_id: 'agent-1',
                requested_model: 'gpt-5',
                effective_model: 'gpt-5',
                provider: 'openai',
                started_at: Date.now(),
                ended_at: null,
                updated_at: Date.now(),
              },
              timeline: {
                events: [
                  {
                    id: 'event-user-1',
                    task_id: 'task-1',
                    agent_id: 'agent-1',
                    category: 'message',
                    source: 'agent_executor',
                    timestamp: Date.now(),
                    subflow_path: [],
                    run_id: null,
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
                    timestamp: Date.now() + 1,
                    subflow_path: [],
                    run_id: null,
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
                    timestamp: Date.now() + 2,
                    subflow_path: [],
                    run_id: null,
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
              child_sessions: [],
            },
          }),
        })
        return
      }

      await route.continue()
    })

    await page.goto(`/workspace/sessions/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId('thread-item-view-event-tool-1')).toBeVisible()
    await expect(page.getByTestId(`chat-message-${assistantMessageId}`)).toBeVisible()
    await expect(
      page.getByText('I found the release notes and summarized the changes in detail.'),
    ).toBeVisible()

    const toolRow = page.getByTestId('thread-item-event-tool-1')
    const assistantRow = page.getByTestId(`chat-message-${assistantMessageId}`)
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
})

test.describe('Session List', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('shows session list state', async ({ page }) => {
    await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible()

    const sessionRows = page.locator('[data-testid^="session-row-"]')
    const backgroundFolders = page.locator('[data-testid^="background-folder-"]')
    const rowCount = await sessionRows.count()
    const folderCount = await backgroundFolders.count()

    if (rowCount > 0) {
      await expect(sessionRows.first()).toBeVisible()
      return
    }

    if (folderCount > 0) {
      await expect(backgroundFolders.first()).toBeVisible()
      return
    }

    await expect(page.getByTestId('session-empty-state')).toBeVisible()
  })

  test('New Session button clears current session', async ({ page }) => {
    const newSessionBtn = page.getByRole('button', { name: 'New Session' })
    await newSessionBtn.click()

    // Should show empty conversation state
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })
})
