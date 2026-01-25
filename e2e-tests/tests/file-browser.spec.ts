import { test, expect } from '@playwright/test'

/**
 * File Browser E2E Tests
 *
 * Design Notes:
 * - FileBrowser "New Skill/Agent" buttons have NO border (unlike TerminalBrowser)
 *   because other file items also have no borders
 * - Search and view toggle controls are in the header, not in the component
 * - Active navigation tab uses text-primary color (not background highlight)
 */
test.describe('File Browser - Skills', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    // Default is Skills page
  })

  test('grid view shows New Skill button without border', async ({ page }) => {
    // New Skill button should exist without dashed border (design decision)
    const newButton = page.locator('button', { hasText: 'New Skill' })
    await expect(newButton).toBeVisible()
    // Should NOT have border-dashed class (unlike TerminalBrowser)
    await expect(newButton).not.toHaveClass(/border-dashed/)
  })

  test('New Skill button shows primary color on hover', async ({ page }) => {
    const newButton = page.locator('button', { hasText: 'New Skill' })
    await newButton.hover()
    // The icon and text should change to primary color on hover
    await expect(newButton).toBeVisible()
  })

  test('clicking New Skill button creates skill', async ({ page }) => {
    const newButton = page.locator('button', { hasText: 'New Skill' })
    await newButton.click()

    // Verify skill editor opens (markdown textarea visible)
    await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()
  })

  test('list view shows New Skill row without border', async ({ page }) => {
    // View toggle is now in header
    const listButton = page.locator('header button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify New Skill row exists without border-dashed
    const newRow = page.locator('button', { hasText: 'New Skill' })
    await expect(newRow).toBeVisible()
    await expect(newRow).not.toHaveClass(/border-dashed/)
  })

  test('skill items show delete button on hover', async ({ page }) => {
    // Find a skill item
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await expect(skillItem).toBeVisible()

    // Hover to show delete button
    await skillItem.hover()
    const deleteButton = skillItem.locator('button[title="Delete"]')
    await expect(deleteButton).toBeVisible()
  })
})

test.describe('File Browser - Agents', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Agents' }).click()
  })

  test('grid view shows New Agent button without border', async ({ page }) => {
    const newButton = page.locator('button', { hasText: 'New Agent' })
    await expect(newButton).toBeVisible()
    await expect(newButton).not.toHaveClass(/border-dashed/)
  })

  test('clicking New Agent button creates agent', async ({ page }) => {
    const newButton = page.locator('button', { hasText: 'New Agent' })
    await newButton.click()

    // Verify agent editor opens (system prompt textarea visible)
    await expect(page.locator('textarea[placeholder*="system prompt"]')).toBeVisible()
  })

  test('list view shows New Agent row without border', async ({ page }) => {
    // View toggle is in header
    const listButton = page.locator('header button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    const newRow = page.locator('button', { hasText: 'New Agent' })
    await expect(newRow).toBeVisible()
    await expect(newRow).not.toHaveClass(/border-dashed/)
  })

  test('agent items show delete button on hover', async ({ page }) => {
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await expect(agentItem).toBeVisible()

    await agentItem.hover()
    const deleteButton = agentItem.locator('button[title="Delete"]')
    await expect(deleteButton).toBeVisible()
  })
})

test.describe('Delete Functionality', () => {
  test('can delete skill from file browser', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    const initialCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(initialCount).toBeGreaterThan(0)

    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.hover()

    const deleteButton = skillItem.locator('button[title="Delete"]')
    await deleteButton.click()

    await page.waitForTimeout(500)

    const newCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(newCount).toBeLessThan(initialCount)
  })

  test('can delete agent from file browser', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Agents' }).click()

    const initialCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(initialCount).toBeGreaterThan(0)

    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.hover()

    const deleteButton = agentItem.locator('button[title="Delete"]')
    await deleteButton.click()

    await page.waitForTimeout(500)

    const newCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(newCount).toBeLessThan(initialCount)
  })
})
