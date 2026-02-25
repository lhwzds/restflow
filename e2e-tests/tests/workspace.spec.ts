import { test, expect } from '@playwright/test'
import { goToWorkspace } from './helpers'

/**
 * Workspace Layout E2E Tests
 *
 * Tests the three-column chat-centric layout:
 * - Left sidebar: Session list with New Session button, agent filter, settings gear
 * - Center: Chat panel with message list and input area
 * - Right: Canvas panel (shown on demand)
 */
test.describe('Workspace Layout', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('renders three-column layout', async ({ page }) => {
    // Left sidebar with session list
    await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible()

    // Center chat area with input
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Bottom bar with settings gear
    const settingsButton = page.getByRole('button', { name: 'Settings' })
    await expect(settingsButton).toBeVisible()
  })

  test('shows dark mode toggle in bottom bar', async ({ page }) => {
    // Dark mode toggle should be next to settings gear
    const bottomBar = page.locator('.border-t.border-border').last()
    const buttons = bottomBar.locator('button')
    // Settings gear + dark mode toggle = at least 2 buttons
    await expect(buttons).toHaveCount(2)
  })

  test('New Session button is visible', async ({ page }) => {
    const newSessionBtn = page.getByRole('button', { name: 'New Session' })
    await expect(newSessionBtn).toBeVisible()
  })

  test('agent filter dropdown is visible', async ({ page }) => {
    // Agent filter select with "All agents" placeholder
    const agentFilter = page.locator('button[role="combobox"]').first()
    await expect(agentFilter).toBeVisible()
  })

  test('chat input area is visible with agent and model selectors', async ({ page }) => {
    // Textarea
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible()

    // Send button
    await expect(page.getByRole('button', { name: 'Send' })).toBeVisible()
  })

  test('send button is disabled when input is empty', async ({ page }) => {
    const sendButton = page.getByRole('button', { name: 'Send' })
    await expect(sendButton).toBeDisabled()
  })

  test('send button enables when text is entered', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Hello')

    const sendButton = page.getByRole('button', { name: 'Send' })
    await expect(sendButton).toBeEnabled()
  })

  test('shows empty state when no messages', async ({ page }) => {
    // Empty state with placeholder text
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })

  test('keyboard hints are hidden in expanded chat mode', async ({ page }) => {
    // In workspace layout, chat is always expanded (isExpanded=true),
    // so keyboard hints (Enter/Shift+Enter) are not shown
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await expect(textarea).toBeVisible()

    // Hints should NOT be visible in expanded mode
    await expect(page.locator('text=Shift+Enter')).not.toBeVisible()
  })
})

test.describe('Session List', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('shows session list state', async ({ page }) => {
    const sessionRows = page.locator('button.w-full.px-3.py-2.text-left')
    const rowCount = await sessionRows.count()

    if (rowCount > 0) {
      await expect(sessionRows.first()).toBeVisible()
      return
    }

    await expect(page.locator('text=No sessions yet')).toBeVisible()
  })

  test('New Session button clears current session', async ({ page }) => {
    const newSessionBtn = page.getByRole('button', { name: 'New Session' })
    await newSessionBtn.click()

    // Should show empty conversation state
    await expect(page.locator('text=Start a new conversation')).toBeVisible()
  })
})
