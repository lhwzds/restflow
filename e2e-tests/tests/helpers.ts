import { expect, Page } from '@playwright/test'

/**
 * Navigate to the workspace and wait for it to load.
 */
export async function goToWorkspace(page: Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem('locale', 'en')
  })
  await page.goto('/workspace')
  await page.waitForLoadState('networkidle')
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
