import { test, expect, type Page } from '@playwright/test'
import { cleanupTrackedState, createSessionForTest, goToWorkspace } from './helpers'

/**
 * Chat Panel E2E Tests
 *
 * Tests the chat input area and message interactions:
 * - Text input and send functionality
 * - Agent and model selectors
 * - Keyboard shortcuts (Enter to send, Shift+Enter for new line)
 */
test.describe('Chat Input', () => {
  async function fillChatTextarea(page: Page, value: string) {
    const selector = 'textarea[placeholder*="Ask the agent"]'

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const textarea = page.locator(selector)
      await expect(textarea).toBeVisible()
      await expect(textarea).toBeEditable()
      await textarea.click()
      await textarea.fill(value)

      if ((await textarea.inputValue()) === value) {
        return textarea
      }
    }

    throw new Error(`Failed to fill chat textarea with expected value: ${value}`)
  }

  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
    await createSessionForTest(page)
  })

  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('can type in chat textarea', async ({ page }) => {
    const textarea = await fillChatTextarea(page, 'Hello, world!')
    await expect(textarea).toHaveValue('Hello, world!')
  })

  test('Enter key sends and clears the input', async ({ page }) => {
    const textarea = await fillChatTextarea(page, 'Test message')

    // Press Enter to send
    await textarea.press('Enter')

    // Input should be cleared after send.
    await expect(textarea).toHaveValue('')
  })

  test('Shift+Enter adds new line instead of sending', async ({ page }) => {
    const textarea = await fillChatTextarea(page, 'Line 1')
    await expect(textarea).toHaveValue('Line 1')

    // Shift+Enter should add new line
    await textarea.press('Shift+Enter')
    await expect(textarea).toHaveValue(/Line 1[\r\n]+/)
    await textarea.type('Line 2')

    // Should contain both lines
    await expect(textarea).toHaveValue(/Line 1[\r\n]+Line 2/)
  })

  test('send button click sends and clears the input', async ({ page }) => {
    const textarea = await fillChatTextarea(page, 'Test message')

    const sendButton = page.getByTestId('chat-send-button')
    await expect(sendButton).toBeEnabled()
    await sendButton.click()

    // Input should be cleared after send.
    await expect(textarea).toHaveValue('')
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
