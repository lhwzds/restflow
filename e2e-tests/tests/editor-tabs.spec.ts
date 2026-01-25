import { test, expect } from '@playwright/test'

/**
 * Editor Tabs E2E Tests
 * Tests the multi-tab editor functionality for Skills, Agents, and Terminals
 */
test.describe('Editor Tabs', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to workspace and wait for it to load
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('double-clicking a skill opens it in a new tab', async ({ page }) => {
    // Find and double-click on a skill item
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Verify editor panel is shown with the tab
    const tab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' })
    await expect(tab).toBeVisible()

    // Verify textarea editor is shown
    const editor = page.locator('textarea[placeholder*="Markdown"]')
    await expect(editor).toBeVisible()
  })

  test('clicking + button shows dropdown menu with options', async ({ page }) => {
    // First open a file to show the editor
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Click the + button
    const plusButton = page.getByRole('button', { name: 'New...' })
    await plusButton.click()

    // Verify dropdown menu items are visible
    await expect(page.getByRole('menuitem', { name: 'New Skill' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'New Agent' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'New Terminal' })).toBeVisible()
  })

  test('New Skill menu item creates a new skill tab', async ({ page }) => {
    // First open a file to show the editor
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Get initial tab count
    const initialTabs = await page.locator('[class*="rounded-t-md"]').count()

    // Click + and select New Skill
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Skill' }).click()

    // Wait for the new tab to appear
    await page.waitForTimeout(500)

    // Verify a new tab was created
    const newTabs = await page.locator('[class*="rounded-t-md"]').count()
    expect(newTabs).toBe(initialTabs + 1)
  })

  test('New Terminal menu item creates a terminal tab', async ({ page }) => {
    // First open a file to show the editor
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Click + and select New Terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()

    // Verify terminal tab is created
    const terminalTab = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' })
    await expect(terminalTab).toBeVisible()

    // Verify terminal UI is shown
    const terminalPrompt = page.locator('text=Terminal ready')
    await expect(terminalPrompt).toBeVisible()

    const commandInput = page.locator('input[placeholder*="command"]')
    await expect(commandInput).toBeVisible()
  })

  test('clicking on a tab switches to that tab', async ({ page }) => {
    // Open first skill
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Create a terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()

    // Terminal should be active - verify terminal UI is shown
    await expect(page.locator('text=Terminal ready')).toBeVisible()

    // Click on the first tab (skill)
    const skillTab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' }).first()
    await skillTab.click()

    // Verify skill editor is now shown (textarea visible)
    const editor = page.locator('textarea[placeholder*="Markdown"]')
    await expect(editor).toBeVisible()

    // Terminal should not be visible
    await expect(page.locator('text=Terminal ready')).not.toBeVisible()
  })

  test('closing a tab removes it and shows another tab', async ({ page }) => {
    // Open first skill
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Create a terminal
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Terminal' }).click()

    // Get tab count
    const initialTabs = await page.locator('[class*="rounded-t-md"]').count()
    expect(initialTabs).toBe(2)

    // Close the terminal tab (it should be active, find its close button)
    const terminalTab = page.locator('[class*="rounded-t-md"]', { hasText: 'Terminal' })
    const closeButton = terminalTab.locator('button').last()
    await closeButton.click()

    // Verify tab count decreased
    const remainingTabs = await page.locator('[class*="rounded-t-md"]').count()
    expect(remainingTabs).toBe(1)

    // Skill editor should now be visible
    const editor = page.locator('textarea[placeholder*="Markdown"]')
    await expect(editor).toBeVisible()
  })

  test('closing all tabs returns to file browser', async ({ page }) => {
    // Open first skill
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Close the tab
    const tab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' })
    const closeButton = tab.locator('button').last()
    await closeButton.click()

    // Verify file browser is shown again
    const fileBrowser = page.locator('text=New Skill').first()
    await expect(fileBrowser).toBeVisible()
  })
})

/**
 * Agent Editor E2E Tests
 * Tests the new markdown-based agent editor with floating config popover
 */
test.describe('Agent Editor', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    // Navigate to Agents tab
    await page.getByRole('button', { name: 'Agents' }).click()
  })

  test('double-clicking an agent opens markdown editor with config button', async ({ page }) => {
    // Find and double-click on an agent item
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.dblclick()

    // Verify agent tab is shown with .agent extension
    const tab = page.locator('[class*="rounded-t-md"]', { hasText: '.agent' })
    await expect(tab).toBeVisible()

    // Verify textarea editor is shown (for system prompt)
    const editor = page.locator('textarea[placeholder*="system prompt"]')
    await expect(editor).toBeVisible()

    // Verify floating config button is visible
    const configButton = page.locator('button').filter({ has: page.locator('svg') }).last()
    await expect(configButton).toBeVisible()
  })

  test('clicking config button opens popover with model, temperature, tools', async ({ page }) => {
    // Open an agent
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.dblclick()

    // Click the config button (Settings icon)
    const configButton = page.locator('button[class*="absolute"]')
    await configButton.click()

    // Verify popover content is visible
    await expect(page.locator('text=Model')).toBeVisible()
    await expect(page.locator('text=Temperature')).toBeVisible()
    await expect(page.locator('text=Tools')).toBeVisible()
  })

  test('can edit system prompt in textarea', async ({ page }) => {
    // Open an agent
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.dblclick()

    // Find the textarea and type in it
    const editor = page.locator('textarea[placeholder*="system prompt"]')
    await editor.fill('You are a helpful assistant.')

    // Verify the content was entered
    await expect(editor).toHaveValue('You are a helpful assistant.')
  })

  test('New Agent creates agent with markdown editor', async ({ page }) => {
    // First open an existing file to show the editor
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.dblclick()

    // Click + and select New Agent
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Agent' }).click()

    // Wait for new tab
    await page.waitForTimeout(500)

    // Verify new agent tab is created
    const agentTabs = page.locator('[class*="rounded-t-md"]', { hasText: '.agent' })
    const count = await agentTabs.count()
    expect(count).toBeGreaterThanOrEqual(1)

    // Verify textarea is visible
    const editor = page.locator('textarea[placeholder*="system prompt"]')
    await expect(editor).toBeVisible()
  })

  test('switching between skill and agent tabs shows correct editor', async ({ page }) => {
    // First go to Skills and open a skill
    await page.getByRole('button', { name: 'Skills' }).click()
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.dblclick()

    // Now open an agent
    await page.getByRole('button', { name: 'New...' }).click()
    await page.getByRole('menuitem', { name: 'New Agent' }).click()
    await page.waitForTimeout(300)

    // Verify agent editor is shown (has config button)
    let configButton = page.locator('button[class*="absolute"]')
    await expect(configButton).toBeVisible()

    // Click on skill tab
    const skillTab = page.locator('[class*="rounded-t-md"]', { hasText: '.md' }).first()
    await skillTab.click()

    // Skill editor should show markdown placeholder
    const skillEditor = page.locator('textarea[placeholder*="Markdown"]')
    await expect(skillEditor).toBeVisible()

    // Config button should not be visible (skill doesn't have it)
    configButton = page.locator('button[class*="absolute"]')
    await expect(configButton).not.toBeVisible()
  })
})
