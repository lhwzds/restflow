import { test, expect } from '@playwright/test'

test.describe('Daemon-served web app', () => {
  test('serves health status over HTTP', async ({ request }) => {
    const response = await request.get('/api/health')

    expect(response.ok()).toBeTruthy()

    const payload = await response.json()
    expect(payload.status).toBe('running')
    expect(payload.protocol_version).toBe('2')
  })

  test('renders workspace from daemon static assets', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('locale', 'en')
    })

    await page.goto('/workspace')
    await page.waitForLoadState('domcontentloaded')

    await expect(page.getByTestId('session-list-new-session')).toBeVisible({
      timeout: 15000,
    })
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Settings' })).toBeVisible()
  })
})
