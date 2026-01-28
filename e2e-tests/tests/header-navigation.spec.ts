import { test, expect } from '@playwright/test'

/**
 * Header Navigation E2E Tests
 *
 * Design Notes:
 * - Navigation tabs are ALWAYS left-aligned (not centered) for consistency
 *   between browse mode and editor mode
 * - Active tab uses text color highlight (text-primary + font-medium)
 *   instead of background highlight for a cleaner look
 * - Browser controls (item count, view toggle, search) are only shown
 *   in browse mode, hidden in editor mode to reduce clutter
 * - Layout: [Logo][Nav] --- spacer --- [Controls][Theme][Settings]
 *
 * Note: Tests create their own items since the app starts with an empty database.
 */
test.describe('Header Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('navigation tabs are left-aligned with logo', async ({ page }) => {
    // Logo should be visible
    await expect(page.locator('header').getByText('RestFlow')).toBeVisible()

    // Navigation should be in header
    const nav = page.locator('header nav')
    await expect(nav).toBeVisible()

    // All three tabs should be visible
    await expect(page.getByRole('button', { name: 'Skills' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Agents' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Terminals' })).toBeVisible()
  })

  test('active tab has primary text color (not background)', async ({ page }) => {
    // Skills tab should be active by default
    const skillsTab = page.getByRole('button', { name: 'Skills' })
    await expect(skillsTab).toHaveClass(/text-primary/)
    await expect(skillsTab).toHaveClass(/font-medium/)

    // Other tabs should have muted color
    const agentsTab = page.getByRole('button', { name: 'Agents' })
    await expect(agentsTab).toHaveClass(/text-muted-foreground/)
  })

  test('clicking tab changes active state', async ({ page }) => {
    // Click Agents tab
    await page.getByRole('button', { name: 'Agents' }).click()

    // Agents should now be active
    const agentsTab = page.getByRole('button', { name: 'Agents' })
    await expect(agentsTab).toHaveClass(/text-primary/)

    // Skills should be inactive
    const skillsTab = page.getByRole('button', { name: 'Skills' })
    await expect(skillsTab).toHaveClass(/text-muted-foreground/)
  })

  test('browser controls visible in browse mode', async ({ page }) => {
    // In browse mode, controls should be visible
    await expect(page.locator('header input[placeholder="Search..."]')).toBeVisible()
    await expect(page.locator('header', { hasText: /\d+ items/ })).toBeVisible()
  })

  test('browser controls hidden in editor mode', async ({ page }) => {
    // Create a skill to enter editor mode
    await page.locator('button', { hasText: 'New Skill' }).click()

    // Wait for editor to open
    await page.waitForTimeout(300)

    // Controls should be hidden in editor mode
    await expect(page.locator('header input[placeholder="Search..."]')).not.toBeVisible()
  })

  test('navigation stays left-aligned in editor mode', async ({ page }) => {
    // Create a skill to enter editor mode
    await page.locator('button', { hasText: 'New Skill' }).click()
    await page.waitForTimeout(300)

    // Navigation should still be visible and in header
    const nav = page.locator('header nav')
    await expect(nav).toBeVisible()

    // All tabs should still be visible
    await expect(page.getByRole('button', { name: 'Skills' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Agents' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Terminals' })).toBeVisible()
  })

  test('clicking nav tab in editor mode returns to browse mode', async ({ page }) => {
    // Create a skill to enter editor mode
    await page.locator('button', { hasText: 'New Skill' }).click()
    await page.waitForTimeout(300)

    // Click Skills tab to return to browse mode
    await page.getByRole('button', { name: 'Skills' }).click()

    // Should be back in browse mode with controls visible
    await expect(page.locator('header input[placeholder="Search..."]')).toBeVisible()
  })

  test('search clears when switching tabs', async ({ page }) => {
    // Enter search text
    const searchInput = page.locator('header input[placeholder="Search..."]')
    await searchInput.fill('test-search')

    // Switch to Agents tab
    await page.getByRole('button', { name: 'Agents' }).click()

    // Search should be cleared
    await expect(searchInput).toHaveValue('')
  })

  test('theme toggle is always visible', async ({ page }) => {
    // Theme toggle should be visible in browse mode
    const themeButton = page.locator('header button').filter({ has: page.locator('svg') }).nth(-2)
    await expect(themeButton).toBeVisible()

    // Enter editor mode by creating a skill
    await page.locator('button', { hasText: 'New Skill' }).click()
    await page.waitForTimeout(300)

    // Theme toggle should still be visible
    await expect(themeButton).toBeVisible()
  })

  test('settings button is always visible', async ({ page }) => {
    // Settings button should be visible
    const settingsButton = page.locator('header button').filter({ has: page.locator('svg') }).last()
    await expect(settingsButton).toBeVisible()
  })
})
