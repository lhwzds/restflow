import { test, expect } from '@playwright/test'
import { goToWorkspace, openSettings, closeSettings } from './helpers'

/**
 * Settings Panel E2E Tests
 *
 * Tests the full-screen settings view:
 * - Left nav with section buttons (Secrets, Auth Profiles, Security, Marketplace)
 * - Right content area showing the selected section
 * - Back button to return to chat layout
 */
test.describe('Settings Panel', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('clicking settings gear opens full-screen settings', async ({ page }) => {
    await openSettings(page)

    // Settings nav items should be visible
    await expect(page.locator('nav button', { hasText: 'Secrets' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'Auth Profiles' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'Security' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'Marketplace' })).toBeVisible()

    // Chat layout should be hidden
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).not.toBeVisible()
  })

  test('back button returns to chat layout', async ({ page }) => {
    await openSettings(page)
    await closeSettings(page)

    // Chat layout should be visible again
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Settings nav should be hidden
    await expect(page.locator('nav button', { hasText: 'Secrets' })).not.toBeVisible()
  })

  test('Secrets is the default active section', async ({ page }) => {
    await openSettings(page)

    // Secrets button should have active styling (font-medium)
    const secretsBtn = page.locator('nav button', { hasText: 'Secrets' })
    await expect(secretsBtn).toHaveClass(/font-medium/)
  })

  test('clicking nav items switches sections', async ({ page }) => {
    await openSettings(page)

    // Click Auth Profiles
    await page.locator('nav button', { hasText: 'Auth Profiles' }).click()
    const authBtn = page.locator('nav button', { hasText: 'Auth Profiles' })
    await expect(authBtn).toHaveClass(/font-medium/)

    // Secrets should no longer be active
    const secretsBtn = page.locator('nav button', { hasText: 'Secrets' })
    await expect(secretsBtn).not.toHaveClass(/font-medium/)
  })

  test('can navigate to Security section', async ({ page }) => {
    await openSettings(page)

    await page.locator('nav button', { hasText: 'Security' }).click()
    const securityBtn = page.locator('nav button', { hasText: 'Security' })
    await expect(securityBtn).toHaveClass(/font-medium/)
  })

  test('can navigate to Marketplace section', async ({ page }) => {
    await openSettings(page)

    await page.locator('nav button', { hasText: 'Marketplace' }).click()
    const marketplaceBtn = page.locator('nav button', { hasText: 'Marketplace' })
    await expect(marketplaceBtn).toHaveClass(/font-medium/)
  })

  test('settings replaces entire chat layout', async ({ page }) => {
    await openSettings(page)

    // Session list should NOT be visible
    await expect(page.getByRole('button', { name: 'New Session' })).not.toBeVisible()

    // Chat panel should NOT be visible
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).not.toBeVisible()

    // Settings nav + content should be visible
    await expect(page.locator('nav button', { hasText: 'Secrets' })).toBeVisible()
  })
})
