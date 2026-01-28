import { test, expect } from '@playwright/test'

/**
 * Terminal Browser E2E Tests
 *
 * Design Notes:
 * - TerminalBrowser "New Terminal" card DOES use dashed border (unlike FileBrowser)
 *   because terminal items are displayed as Card components with borders
 * - Search and view toggle controls are in the header (shared with Skills/Agents)
 * - Stopped terminals auto-restart when clicked for better UX
 */
test.describe('Terminal Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Terminals' }).click()
  })

  test('header shows controls when in browse mode', async ({ page }) => {
    // Verify search input exists in header
    await expect(page.locator('header input[placeholder="Search..."]')).toBeVisible()

    // Verify view toggle buttons exist in header
    const viewToggle = page.locator('header .flex.gap-0\\.5.border.rounded-md')
    await expect(viewToggle).toBeVisible()

    // Verify item count is displayed
    await expect(page.locator('header', { hasText: /\d+ items/ })).toBeVisible()
  })

  test('search filters terminals by name', async ({ page }) => {
    // Create a terminal first - in grid view, New Terminal is a Card (div), not a button
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Search for nonexistent name (search is in header)
    await page.locator('header input[placeholder="Search..."]').fill('nonexistent-xyz')
    await expect(page.locator('header', { hasText: '0 items' })).toBeVisible()

    // Clear search to show all
    await page.locator('header input[placeholder="Search..."]').fill('')
    await expect(page.locator('header', { hasText: /\d+ items/ })).toBeVisible()
  })

  test('view toggle switches between grid and list', async ({ page }) => {
    // Create a terminal to have content - in grid view, New Terminal is a Card (div), not a button
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Default should be Grid view - grid layout should be visible
    const gridLayout = page.locator('.grid.grid-cols-2')
    await expect(gridLayout).toBeVisible()

    // Click List view button in header (first button in toggle group)
    const listButton = page.locator('header button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // List view should now be visible (space-y-1 layout)
    await expect(page.locator('.space-y-1')).toBeVisible()
  })

  test('New Terminal card has dashed border (design: matches card style)', async ({ page }) => {
    // TerminalBrowser uses dashed border because terminal items are cards with borders
    // This is different from FileBrowser which has no border
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await expect(newCard).toBeVisible()
  })

  test('clicking New Terminal card creates terminal', async ({ page }) => {
    // In grid view, New Terminal is a Card (div), not a button
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()

    // Verify terminal was created (editor opens)
    // In web mode (not Tauri), terminal shows error message
    await expect(page.locator('text=Terminal requires Tauri desktop app')).toBeVisible()
  })

  test('list view New Terminal row has dashed border', async ({ page }) => {
    // Switch to List view (button is in header)
    const listButton = page.locator('header button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify New Terminal row exists with dashed border
    const newRow = page.locator('button.border-dashed', { hasText: 'New Terminal' })
    await expect(newRow).toBeVisible()
  })

  test('terminal items show delete button on hover', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Find terminal card and hover
    const terminalCard = page.locator('.group.relative.cursor-pointer', { hasText: /Terminal \d+/ }).first()
    await terminalCard.hover()

    // Verify delete button appears
    const deleteButton = page.locator('button[title="Delete terminal"]').first()
    await expect(deleteButton).toBeVisible()
  })

  test('running terminal shows stop button on hover', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Find terminal card and hover
    const terminalCard = page.locator('.group.relative.cursor-pointer', { hasText: /Terminal \d+/ }).first()
    await terminalCard.hover()

    // Verify stop button appears for running terminal
    const stopButton = page.locator('button[title="Stop terminal"]').first()
    await expect(stopButton).toBeVisible()
  })

  test('clicking stop button stops the terminal', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Find terminal card and hover
    const terminalCard = page.locator('.group.relative.cursor-pointer', { hasText: /Terminal \d+/ }).first()
    await terminalCard.hover()

    // Click stop button
    const stopButton = page.locator('button[title="Stop terminal"]').first()
    await stopButton.click()
    await page.waitForTimeout(500)

    // After stopping, hover again to check stop button is gone
    await terminalCard.hover()

    // Stop button should not be visible for stopped terminal
    await expect(page.locator('button[title="Stop terminal"]')).not.toBeVisible()
  })

  test('settings button opens terminal settings dialog', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Find terminal card and hover
    const terminalCard = page.locator('.group.relative.cursor-pointer', { hasText: /Terminal \d+/ }).first()
    await terminalCard.hover()

    // Click settings button
    const settingsButton = page.locator('button[title="Terminal settings"]').first()
    await settingsButton.click()

    // Verify settings dialog opens
    await expect(page.getByRole('dialog')).toBeVisible()
    await expect(page.getByText('Terminal Settings')).toBeVisible()

    // Verify settings fields are present (using text instead of label since label isn't associated)
    await expect(page.getByText('Working Directory')).toBeVisible()
    await expect(page.getByText('Startup Command')).toBeVisible()
  })

  test('settings dialog shows restart prompt after saving for running terminal', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Find terminal card and hover
    const terminalCard = page.locator('.group.relative.cursor-pointer', { hasText: /Terminal \d+/ }).first()
    await terminalCard.hover()

    // Click settings button
    const settingsButton = page.locator('button[title="Terminal settings"]').first()
    await settingsButton.click()
    await page.waitForTimeout(200)

    // Fill in a setting (use placeholder since label isn't associated)
    await page.getByPlaceholder('e.g., ~/projects').fill('~/projects')

    // Click save
    await page.getByRole('button', { name: 'Save' }).click()
    await page.waitForTimeout(300)

    // Verify restart prompt appears in the same dialog
    await expect(page.getByText('Settings saved!')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Later' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Restart Now' })).toBeVisible()
  })

  test('list view shows stop button for running terminals', async ({ page }) => {
    // Create a terminal first
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Switch to List view
    const listButton = page.locator('header button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()
    await page.waitForTimeout(200)

    // Find terminal row and hover
    const terminalRow = page.locator('button', { hasText: /Terminal \d+/ }).first()
    await terminalRow.hover()

    // Verify stop button appears
    const stopButton = page.locator('button[title="Stop terminal"]').first()
    await expect(stopButton).toBeVisible()
  })
})
