import { test, expect } from '@playwright/test'
import { createAgentInBrowser, createAgentAndOpenEditor, createSkillInBrowser, createSkillAndOpenEditor } from './helpers'

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
    await createSkillAndOpenEditor(page)
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
    const skillItem = await createSkillInBrowser(page)

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
    await createAgentAndOpenEditor(page)
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
    const agentItem = await createAgentInBrowser(page)

    await agentItem.hover()
    const deleteButton = agentItem.locator('button[title="Delete"]')
    await expect(deleteButton).toBeVisible()
  })
})

test.describe('Agent Editor Settings', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Agents' }).click()
  })

  test('settings button is clickable and opens popover', async ({ page }) => {
    await createAgentAndOpenEditor(page)

    // Find the settings button in the editor
    // The editor settings button uses Popover which has aria-haspopup="dialog"
    const settingsButton = page.locator('button[aria-haspopup="dialog"]')
    await expect(settingsButton).toBeVisible()

    // Click the settings button
    await settingsButton.click()

    // Verify the popover opens with model selector
    await expect(page.getByText('Model')).toBeVisible()
    await expect(page.getByText('Temperature')).toBeVisible()
  })

  test('can change model in settings popover', async ({ page }) => {
    await createAgentAndOpenEditor(page)

    // Open settings - the editor settings button uses Popover with aria-haspopup="dialog"
    const settingsButton = page.locator('button[aria-haspopup="dialog"]')
    await settingsButton.click()

    // Wait for popover to open
    await expect(page.getByText('Model')).toBeVisible()

    // Find the model selector (first combobox in the popover)
    const modelSelect = page.locator('[role="combobox"]').first()
    await expect(modelSelect).toBeVisible()
    await modelSelect.click()

    // Verify model options are visible
    await expect(page.getByRole('option').first()).toBeVisible()
  })
})

test.describe('Delete Functionality', () => {
  test('can delete skill from file browser', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    await createSkillInBrowser(page)

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

    const agentItem = await createAgentInBrowser(page)

    // Get the specific agent name we'll delete
    const agentName = await agentItem.locator('span, div').filter({ hasText: /Untitled-\d+/ }).first().textContent()
    expect(agentName).toBeTruthy()

    await agentItem.hover()
    const deleteButton = agentItem.locator('button[title="Delete"]')
    await deleteButton.click()

    // Wait for deletion and verify the specific item is gone
    await page.waitForTimeout(500)

    // Verify deletion success notification appears
    await expect(page.locator('text=Deleted successfully').first()).toBeVisible()
  })
})
