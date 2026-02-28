import { test, expect } from '@playwright/test'
import { goToWorkspace } from './helpers'

test.describe('Chat streaming', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('renders streamed assistant output with tool call steps', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('stream test')
    await page.getByRole('button', { name: 'Send' }).click()

    await expect(page.locator('text=web_search')).toBeVisible()
    await expect(page.locator('text=Stream response for: stream test')).toBeVisible()
  })

  test('keeps tool steps visible after stream completes using persisted execution', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('persisted steps test')
    await page.getByRole('button', { name: 'Send' }).click()

    await expect(page.locator('text=Stream response for: persisted steps test')).toBeVisible()
    await expect(page.getByRole('button', { name: 'View' })).toHaveCount(0, { timeout: 15000 })
    await expect(page.locator('text=web_search')).toBeVisible()
  })
})
