import { expect, test } from '@playwright/test'

test.describe('Document title localization', () => {
  test('uses Chinese brand title by default', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.removeItem('locale')
    })
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    await expect.poll(async () => page.title()).toBe('浮流 RestFlow')
  })

  test('uses English brand title when locale is en', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('locale', 'en')
    })
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    await expect.poll(async () => page.title()).toBe('RestFlow')
  })
})
