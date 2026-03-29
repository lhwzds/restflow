import { test, expect } from '@playwright/test'
import { goToWorkspace, openSettings, closeSettings } from './helpers'

/**
 * Settings Panel E2E Tests
 *
 * Tests the full-screen settings view:
 * - Left nav with section buttons (Secrets, Auth Profiles)
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
    await expect(page.locator('nav button', { hasText: 'Hooks' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'Marketplace' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'Memory' })).toBeVisible()
    await expect(page.locator('nav button', { hasText: 'System' })).toBeVisible()

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

    // Secrets button should be active
    const secretsBtn = page.locator('nav button', { hasText: 'Secrets' })
    await expect(secretsBtn).toHaveAttribute('data-active', 'true')
  })

  test('clicking nav items switches sections', async ({ page }) => {
    await openSettings(page)

    // Click Auth Profiles
    await page.locator('nav button', { hasText: 'Auth Profiles' }).click()
    const authBtn = page.locator('nav button', { hasText: 'Auth Profiles' })
    await expect(authBtn).toHaveAttribute('data-active', 'true')

    // Secrets should no longer be active
    const secretsBtn = page.locator('nav button', { hasText: 'Secrets' })
    await expect(secretsBtn).toHaveAttribute('data-active', 'false')

    // Click Hooks
    await page.locator('nav button', { hasText: 'Hooks' }).click()
    const hooksBtn = page.locator('nav button', { hasText: 'Hooks' })
    await expect(hooksBtn).toHaveAttribute('data-active', 'true')

    // Click Marketplace
    await page.locator('nav button', { hasText: 'Marketplace' }).click()
    const marketplaceBtn = page.locator('nav button', { hasText: 'Marketplace' })
    await expect(marketplaceBtn).toHaveAttribute('data-active', 'true')

    // Click Memory
    await page.locator('nav button', { hasText: 'Memory' }).click()
    const memoryBtn = page.locator('nav button', { hasText: 'Memory' })
    await expect(memoryBtn).toHaveAttribute('data-active', 'true')

    // Click System
    await page.locator('nav button', { hasText: 'System' }).click()
    const systemBtn = page.locator('nav button', { hasText: 'System' })
    await expect(systemBtn).toHaveAttribute('data-active', 'true')
  })

  test('hooks section exposes add hook action', async ({ page }) => {
    await openSettings(page)
    await page.locator('nav button', { hasText: 'Hooks' }).click()

    const addHookButton = page.getByRole('button', { name: 'Add Hook' })
    await expect(addHookButton).toBeVisible()
  })

  test('marketplace section exposes filter controls', async ({ page }) => {
    await openSettings(page)
    await page.locator('nav button', { hasText: 'Marketplace' }).click()

    await expect(page.getByPlaceholder('Search skills by name, tag, or author')).toBeVisible()
    await expect(page.locator('label').filter({ hasText: /^Category$/ })).toBeVisible()
    await expect(page.locator('label').filter({ hasText: /^Sort$/ })).toBeVisible()
  })

  test('memory section exposes session and export actions', async ({ page }) => {
    await openSettings(page)
    await page.locator('nav button', { hasText: 'Memory' }).click()

    await expect(page.getByRole('button', { name: 'Delete Session' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Export Markdown' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Copy' })).toBeVisible()
  })

  test('system section exposes config and utility actions', async ({ page }) => {
    await openSettings(page)
    await page.locator('nav button', { hasText: 'System' }).click()

    await expect(page.getByRole('button', { name: 'Load Config' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Save Config' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Check' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Import Skill' })).toBeVisible()
  })

  test('settings replaces entire chat layout', async ({ page }) => {
    await openSettings(page)

    // Session list should NOT be visible
    await expect(page.getByTestId('session-list-new-session')).not.toBeVisible()

    // Chat panel should NOT be visible
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).not.toBeVisible()

    // Settings nav + content should be visible
    await expect(page.locator('nav button', { hasText: 'Secrets' })).toBeVisible()
  })
})
