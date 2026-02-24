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
})
