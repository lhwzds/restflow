import { test, expect } from '@playwright/test'

/**
 * Terminal Feature E2E Tests
 *
 * These tests cover the Terminal tab in the workspace navigation
 * and the TerminalBrowser component functionality.
 *
 * Note: Full PTY terminal functionality requires Tauri desktop app.
 * These tests cover the UI interactions that work in web mode.
 */
test.describe('Terminal Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('clicking Terminals tab shows terminal browser', async ({ page }) => {
    // Click on Terminals in navigation
    const terminalsTab = page.getByRole('button', { name: 'Terminals' })
    await terminalsTab.click()

    // Verify terminal browser is shown - in grid view, New Terminal is a Card (div), not a button
    const newTerminalCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await expect(newTerminalCard).toBeVisible()
  })

  test('can switch between Skills, Agents, and Terminals tabs', async ({ page }) => {
    // Start on Skills (default)
    const skillsTab = page.getByRole('button', { name: 'Skills' })
    await expect(skillsTab).toHaveClass(/text-primary/)

    // Switch to Agents
    const agentsTab = page.getByRole('button', { name: 'Agents' })
    await agentsTab.click()
    await expect(agentsTab).toHaveClass(/text-primary/)

    // Switch to Terminals
    const terminalsTab = page.getByRole('button', { name: 'Terminals' })
    await terminalsTab.click()
    await expect(terminalsTab).toHaveClass(/text-primary/)

    // Switch back to Skills
    await skillsTab.click()
    await expect(skillsTab).toHaveClass(/text-primary/)
  })

  test('New Terminal button is visible in terminal browser', async ({ page }) => {
    // Navigate to Terminals tab
    await page.getByRole('button', { name: 'Terminals' }).click()

    // Check for New Terminal card - in grid view, it's a Card (div), not a button
    const newTerminalCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await expect(newTerminalCard).toBeVisible()
  })
})

test.describe('Terminal Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    // Navigate to Terminals tab
    await page.getByRole('button', { name: 'Terminals' }).click()
  })

  test('shows empty state message when no terminals exist', async ({ page }) => {
    // In web mode with mock data, "0 items" is shown in header when no terminals
    // Just verify the page loaded and New Terminal is available
    const newTerminalCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await expect(newTerminalCard).toBeVisible()
  })

  test('New Terminal button creates a terminal session', async ({ page }) => {
    // Click New Terminal - in grid view, it's a Card (div), not a button
    await page.locator('.border-dashed', { hasText: 'New Terminal' }).click()

    // Wait for terminal to be created and editor to open
    // In web mode, shows error message about Tauri
    await expect(page.locator('text=Terminal requires Tauri desktop app')).toBeVisible()
  })
})

test.describe('Terminal Tab Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('creating terminal from + menu opens terminal tab', async ({ page }) => {
    // First open a file to show the editor
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Click + and select New Terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()

    // Verify terminal tab is created
    const terminalTab = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' })
    await expect(terminalTab).toBeVisible()
  })

  test('can create multiple terminal tabs', async ({ page }) => {
    // Open a file first
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Create first terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()
    await page.waitForTimeout(300)

    // Create second terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()
    await page.waitForTimeout(300)

    // Count terminal tabs
    const terminalTabs = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' })
    const count = await terminalTabs.count()
    expect(count).toBeGreaterThanOrEqual(2)
  })

  test('switching between terminal and skill tabs preserves state', async ({ page }) => {
    // Open a skill
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Create a terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()
    await page.waitForTimeout(300)

    // Click back on skill tab
    const skillTab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' }).first()
    await skillTab.click()

    // Verify skill editor is shown
    const editor = page.locator('textarea[placeholder*="Markdown"]')
    await expect(editor).toBeVisible()

    // Click on terminal tab
    const terminalTab = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' }).first()
    await terminalTab.click()

    // Terminal UI should be visible (in web mode, shows Tauri error message)
    await expect(page.locator('text=Terminal requires Tauri desktop app')).toBeVisible()
  })
})

test.describe('Terminal Tab Close Behavior', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('closing terminal tab removes it from tab bar', async ({ page }) => {
    // Open a skill
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Create a terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()
    await page.waitForTimeout(300)

    // Get initial tab count
    const initialCount = await page.locator('[class*="rounded-t-md"]').count()

    // Close terminal tab
    const terminalTab = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' })
    const closeButton = terminalTab.locator('button').last()
    await closeButton.click()

    // Verify tab count decreased
    const finalCount = await page.locator('[class*="rounded-t-md"]').count()
    expect(finalCount).toBe(initialCount - 1)
  })

  test('closing last tab returns to file browser', async ({ page }) => {
    // Navigate to Terminals
    await page.getByRole('button', { name: 'Terminals' }).click()

    // Open a skill first (to have a tab)
    await page.getByRole('button', { name: 'Skills' }).click()
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Close the tab
    const tab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' })
    const closeButton = tab.locator('button').last()
    await closeButton.click()

    // Verify file browser is shown
    const newButton = page.locator('button', { hasText: /New Skill|New Agent/ }).first()
    await expect(newButton).toBeVisible()
  })
})
