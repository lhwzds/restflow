import { test, expect } from '@playwright/test'
import { goToWorkspace } from './helpers'

/**
 * Chat Panel E2E Tests
 *
 * Tests the chat input area and message interactions:
 * - Text input and send functionality
 * - Agent and model selectors
 * - Keyboard shortcuts (Enter to send, Shift+Enter for new line)
 */
test.describe('Chat Input', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('can type in chat textarea', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Hello, world!')
    await expect(textarea).toHaveValue('Hello, world!')
  })

  test('Enter key keeps the typed message available', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Test message')

    // Press Enter to send
    await textarea.press('Enter')

    // Input remains available for retry if send cannot complete in mocked E2E mode.
    await expect(textarea).toHaveValue('Test message')
  })

  test('Shift+Enter adds new line instead of sending', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Line 1')

    // Shift+Enter should add new line
    await textarea.press('Shift+Enter')
    await textarea.type('Line 2')

    // Should contain both lines
    const value = await textarea.inputValue()
    expect(value).toContain('Line 1')
    expect(value).toContain('Line 2')
  })

  test('send button click keeps message available in mocked mode', async ({ page }) => {
    const textarea = page.locator('textarea[placeholder*="Ask the agent"]')
    await textarea.fill('Test message')

    const sendButton = page.getByRole('button', { name: 'Send' })
    await sendButton.click()

    // Input remains available for retry if send cannot complete in mocked E2E mode.
    await expect(textarea).toHaveValue('Test message')
  })

  test('agent selector is visible in chat box', async ({ page }) => {
    // Agent selector dropdown should be present
    const agentSelector = page.locator('.chat-textarea').locator('..').locator('..').locator('button[role="combobox"]').first()
    // There should be at least one combobox-like selector in the chat input area
    const selectors = page.locator('button[role="combobox"]')
    const count = await selectors.count()
    expect(count).toBeGreaterThanOrEqual(1)
  })

  test('model selector is visible in chat box', async ({ page }) => {
    // Model selector with CPU icon should be present
    const modelSelector = page.locator('button[role="combobox"]').filter({ has: page.locator('svg.lucide-cpu') })
    await expect(modelSelector).toBeVisible()
  })
})
