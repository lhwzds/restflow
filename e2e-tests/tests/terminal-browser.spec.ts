import { test, expect } from '@playwright/test'

/**
 * Terminal Browser E2E Tests
 * Tests the terminal browser UI including search, view toggle, and card creation
 */
test.describe('Terminal Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Terminals' }).click()
  })

  test('displays toolbar with search and view toggle', async ({ page }) => {
    // Verify search input exists
    await expect(page.locator('input[placeholder="Search..."]')).toBeVisible()

    // Verify view toggle buttons exist
    const viewToggle = page.locator('.flex.gap-0\\.5.border.rounded-md')
    await expect(viewToggle).toBeVisible()

    // Verify New Terminal button exists
    await expect(page.getByRole('button', { name: 'New Terminal' })).toBeVisible()
  })

  test('search filters terminals by name', async ({ page }) => {
    // Create a terminal first
    await page.getByRole('button', { name: 'New Terminal' }).click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Get initial count
    const initialCount = await page.locator('text=/\\d+ items/').textContent()
    expect(initialCount).toContain('items')

    // Search for nonexistent name
    await page.locator('input[placeholder="Search..."]').fill('nonexistent-xyz')
    await expect(page.locator('text=0 items')).toBeVisible()

    // Clear search to show all
    await page.locator('input[placeholder="Search..."]').fill('')
    await expect(page.locator('text=/\\d+ items/')).toBeVisible()
  })

  test('view toggle switches between grid and list', async ({ page }) => {
    // Create a terminal to have content
    await page.getByRole('button', { name: 'New Terminal' }).click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Default should be Grid view - grid layout should be visible
    const gridLayout = page.locator('.grid.grid-cols-2')
    await expect(gridLayout).toBeVisible()

    // Click List view button (first button in toggle group)
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // List view should now be visible (space-y-1 layout)
    await expect(page.locator('.space-y-1')).toBeVisible()

    // Grid layout should not be visible
    await expect(gridLayout).not.toBeVisible()
  })

  test('clicking New Terminal card creates terminal', async ({ page }) => {
    // Find the dashed card with "New Terminal" text and click it
    const newCard = page.locator('button', { hasText: 'New Terminal' }).last()
    await newCard.click()

    // Verify terminal was created (editor opens)
    await expect(page.locator('text=Terminal ready')).toBeVisible()
  })

  test('list view shows New Terminal row', async ({ page }) => {
    // Switch to List view
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify New Terminal row exists in list view (has border-dashed class)
    const newRow = page.locator('button.border-dashed', { hasText: 'New Terminal' })
    await expect(newRow).toBeVisible()
  })

  test('list view terminal items have correct structure', async ({ page }) => {
    // Create a terminal first
    await page.getByRole('button', { name: 'New Terminal' }).click()
    await page.waitForTimeout(500)

    // Return to browser
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForTimeout(300)

    // Switch to List view
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify terminal row has status indicator, name, and delete button on hover
    const terminalRow = page.locator('button.rounded-lg', { hasText: /Terminal \d+/ }).first()
    await expect(terminalRow).toBeVisible()

    // Hover to show delete button
    await terminalRow.hover()
    const deleteButton = terminalRow.locator('button[title="Delete terminal"]')
    await expect(deleteButton).toBeVisible()
  })
})
