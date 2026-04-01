import { expect, Page } from '@playwright/test'

type SessionLike = {
  id: string
  agent_id?: string
  model?: string
}

type CreateSessionRequest = {
  agent_id?: string | null
  model: string
  name?: string | null
  skill_id?: string | null
}

type TrackedState = {
  sessionIds: Set<string>
  backgroundTaskIds: Set<string>
}

type BackgroundAgentDeleteResult = {
  id: string
  deleted: boolean
}

const trackedState = new WeakMap<Page, TrackedState>()

function getTrackedState(page: Page): TrackedState {
  const existing = trackedState.get(page)
  if (existing) {
    return existing
  }

  const created: TrackedState = {
    sessionIds: new Set(),
    backgroundTaskIds: new Set(),
  }
  trackedState.set(page, created)
  return created
}

function isNotFoundError(error: unknown): boolean {
  return error instanceof Error && /not found/i.test(error.message)
}

function rememberSessionId(page: Page, sessionId: string) {
  getTrackedState(page).sessionIds.add(sessionId)
}

function rememberBackgroundTaskId(page: Page, taskId: string) {
  getTrackedState(page).backgroundTaskIds.add(taskId)
}

/**
 * Navigate to the workspace and wait for it to load.
 */
export async function goToWorkspace(page: Page) {
  await ensureDefaultE2eProvider()
  await page.addInitScript(() => {
    window.localStorage.setItem('locale', 'en')
  })
  await page.goto('/workspace')
  await page.waitForLoadState('domcontentloaded')
  await expect(page.getByTestId('session-list-new-session')).toBeVisible({
    timeout: 15000,
  })
}

async function ensureDefaultE2eProvider() {
  await requestIpcDirect({
    type: 'SetSecret',
    data: {
      key: 'OPENAI_API_KEY',
      value: 'e2e-openai-key',
      description: 'Seeded E2E provider secret',
    },
  })
}

/**
 * Open the full-screen Settings panel by clicking the gear icon.
 */
export async function openSettings(page: Page) {
  const settingsButton = page.getByRole('button', { name: 'Settings' })
  await settingsButton.click()
  // Wait for settings left nav to appear
  await expect(page.locator('nav button', { hasText: 'Secrets' })).toBeVisible()
}

/**
 * Close Settings and return to the chat layout.
 */
export async function closeSettings(page: Page) {
  const backButton = page.getByRole('button', { name: 'Back to workspace' })
  await backButton.click()
  // Wait for session list to appear
  await expect(page.getByTestId('session-list-new-session')).toBeVisible()
}

export async function requestIpc<T>(page: Page, request: Record<string, unknown>): Promise<T> {
  return page.evaluate(async (payload) => {
    const response = await fetch('/api/request', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      body: JSON.stringify(payload),
    })

    if (!response.ok) {
      throw new Error((await response.text()) || `HTTP ${response.status}`)
    }

    const envelope = (await response.json()) as
      | { response_type: 'Success' | 'success'; data: T }
      | { response_type: 'Error' | 'error'; data: { message?: string } }
      | { response_type: 'Pong' | 'pong'; data: unknown }
      | { response_type: string; data: unknown }

    const responseType = envelope.response_type.toLowerCase()

    if (responseType === 'success') {
      return envelope.data
    }

    if (responseType === 'error') {
      throw new Error(envelope.data?.message || 'Daemon request failed')
    }

    throw new Error(`Unexpected daemon response: ${envelope.response_type}`)
  }, request)
}

async function requestIpcDirect<T>(request: Record<string, unknown>): Promise<T> {
  const baseUrl = process.env.BASE_URL?.trim()
  if (!baseUrl) {
    throw new Error('BASE_URL is required for direct E2E IPC requests')
  }

  const response = await fetch(new URL('/api/request', baseUrl), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!response.ok) {
    throw new Error((await response.text()) || `HTTP ${response.status}`)
  }

  const envelope = (await response.json()) as
    | { response_type: 'Success' | 'success'; data: T }
    | { response_type: 'Error' | 'error'; data: { message?: string } }
    | { response_type: 'Pong' | 'pong'; data: unknown }
    | { response_type: string; data: unknown }

  const responseType = envelope.response_type.toLowerCase()

  if (responseType === 'success') {
    return envelope.data
  }

  if (responseType === 'error') {
    throw new Error(envelope.data?.message || 'Daemon request failed')
  }

  throw new Error(`Unexpected daemon response: ${envelope.response_type}`)
}

async function deleteBackgroundTaskDirect(taskId: string): Promise<void> {
  const result = await requestIpcDirect<BackgroundAgentDeleteResult>({
    type: 'DeleteBackgroundAgent',
    data: { id: taskId },
  })

  if (!result.deleted) {
    throw new Error(`Failed to delete background task ${taskId}`)
  }
}

export function trackCreatedSession(page: Page, sessionId: string) {
  rememberSessionId(page, sessionId)
}

export function trackCreatedBackgroundTask(page: Page, taskId: string) {
  rememberBackgroundTaskId(page, taskId)
}

export async function createSessionForTest(page: Page): Promise<string> {
  await Promise.all([
    page.waitForURL(/\/workspace\/c\/[^/?#]+$/, { timeout: 15000 }),
    page.getByTestId('session-list-new-session').click(),
  ])

  await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible({
    timeout: 15000,
  })

  const sessionMatch = page.url().match(/\/workspace\/c\/([^/?#]+)/)
  const sessionId = sessionMatch?.[1] ?? null
  if (!sessionId) {
    throw new Error('Failed to read the new workspace session id from the URL')
  }

  rememberSessionId(page, sessionId)
  return sessionId
}

export async function createApiSessionForTest(
  page: Page,
  request: CreateSessionRequest,
): Promise<SessionLike> {
  const session = await requestIpc<SessionLike>(page, {
    type: 'CreateSession',
    data: {
      agent_id: request.agent_id ?? null,
      model: request.model,
      name: request.name ?? null,
      skill_id: request.skill_id ?? null,
    },
  })

  rememberSessionId(page, session.id)
  return session
}

export async function cleanupTrackedState(page: Page) {
  const state = trackedState.get(page)
  if (!state) {
    return
  }

  const sessionIds = [...state.sessionIds].reverse()
  const backgroundTaskIds = [...state.backgroundTaskIds].reverse()
  trackedState.delete(page)

  const cleanupErrors: string[] = []

  for (const taskId of backgroundTaskIds) {
    try {
      await deleteBackgroundTaskDirect(taskId)
    } catch (error) {
      if (!isNotFoundError(error)) {
        cleanupErrors.push(
          `Failed to delete background task ${taskId}: ${
            error instanceof Error ? error.message : String(error)
          }`,
        )
      }
    }
  }

  for (const sessionId of sessionIds) {
    try {
      await requestIpcDirect<{ deleted?: boolean }>({
        type: 'DeleteSession',
        data: { id: sessionId },
      })
    } catch (error) {
      if (!isNotFoundError(error)) {
        cleanupErrors.push(
          `Failed to delete session ${sessionId}: ${
            error instanceof Error ? error.message : String(error)
          }`,
        )
      }
    }
  }

  if (cleanupErrors.length > 0) {
    throw new Error(`Failed to clean up E2E state:\n${cleanupErrors.join('\n')}`)
  }
}
