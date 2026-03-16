import { expect, Page } from '@playwright/test'

/**
 * Navigate to the workspace and wait for it to load.
 */
export async function goToWorkspace(page: Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem('locale', 'en')
  })
  await page.goto('/workspace')
  await page.waitForLoadState('domcontentloaded')
  await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible({
    timeout: 15000,
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
  await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible()
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
