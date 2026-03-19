import { expect, test } from '@playwright/test'

test.describe('Document title localization', () => {
  test('uses brand title by default', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('domcontentloaded')
    await expect(page.getByAltText('RestFlow logo').first()).toBeVisible({
      timeout: 15000,
    })

    await expect.poll(async () => page.title()).toBe('RestFlow')
  })

  test('uses brand title when locale is en', async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('locale', 'en')
    })
    await page.goto('/workspace')
    await page.waitForLoadState('domcontentloaded')
    await expect(page.getByAltText('RestFlow logo').first()).toBeVisible({
      timeout: 15000,
    })

    await expect.poll(async () => page.title()).toBe('RestFlow')
  })
})
