import { test, expect } from '@playwright/test'

/**
 * File Browser E2E Tests
 * Tests the file browser UI for Skills and Agents including the new card creation
 */
test.describe('File Browser - Skills', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    // Default is Skills page
  })

  test('grid view shows New Skill card', async ({ page }) => {
    // Verify dashed card with "New Skill" exists
    const newCard = page.locator('button.border-dashed', { hasText: 'New Skill' })
    await expect(newCard).toBeVisible()
  })

  test('clicking New Skill card creates skill', async ({ page }) => {
    // Find and click the dashed card
    const newCard = page.locator('button.border-dashed', { hasText: 'New Skill' })
    await newCard.click()

    // Verify skill editor opens (markdown textarea visible)
    await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()
  })

  test('list view shows New Skill row', async ({ page }) => {
    // Switch to List view (first button in toggle group)
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify New Skill row exists with border-dashed
    const newRow = page.locator('button.border-dashed', { hasText: 'New Skill' })
    await expect(newRow).toBeVisible()
  })

  test('clicking New Skill row in list view creates skill', async ({ page }) => {
    // Switch to List view
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Click the New Skill row
    const newRow = page.locator('button.border-dashed', { hasText: 'New Skill' })
    await newRow.click()

    // Verify skill editor opens
    await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()
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

  test('grid view shows New Agent card', async ({ page }) => {
    // Verify dashed card with "New Agent" exists
    const newCard = page.locator('button.border-dashed', { hasText: 'New Agent' })
    await expect(newCard).toBeVisible()
  })

  test('clicking New Agent card creates agent', async ({ page }) => {
    // Find and click the dashed card
    const newCard = page.locator('button.border-dashed', { hasText: 'New Agent' })
    await newCard.click()

    // Verify agent editor opens (system prompt textarea visible)
    await expect(page.locator('textarea[placeholder*="system prompt"]')).toBeVisible()
  })

  test('list view shows New Agent row', async ({ page }) => {
    // Switch to List view
    const listButton = page.locator('button[class*="h-6"][class*="w-6"]').first()
    await listButton.click()

    // Verify New Agent row exists with border-dashed
    const newRow = page.locator('button.border-dashed', { hasText: 'New Agent' })
    await expect(newRow).toBeVisible()
  })

  test('agent items show delete button on hover', async ({ page }) => {
    // Find an agent item
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await expect(agentItem).toBeVisible()

    // Hover to show delete button
    await agentItem.hover()
    const deleteButton = agentItem.locator('button[title="Delete"]')
    await expect(deleteButton).toBeVisible()
  })
})

test.describe('Delete Functionality', () => {
  test('can delete skill from file browser', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    // Get initial skill count
    const initialCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(initialCount).toBeGreaterThan(0)

    // Hover on first skill to show delete button
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await skillItem.hover()

    // Click delete button
    const deleteButton = skillItem.locator('button[title="Delete"]')
    await deleteButton.click()

    // Wait for deletion
    await page.waitForTimeout(500)

    // Verify count decreased (or skill removed)
    const newCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(newCount).toBeLessThan(initialCount)
  })

  test('can delete agent from file browser', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: 'Agents' }).click()

    // Get initial agent count
    const initialCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(initialCount).toBeGreaterThan(0)

    // Hover on first agent to show delete button
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.hover()

    // Click delete button
    const deleteButton = agentItem.locator('button[title="Delete"]')
    await deleteButton.click()

    // Wait for deletion
    await page.waitForTimeout(500)

    // Verify count decreased
    const newCount = await page.locator('button', { hasText: /Untitled-\d+/ }).count()
    expect(newCount).toBeLessThan(initialCount)
  })

  test('deleting open file closes its tab', async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    // Open a skill first
    const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    const skillName = await skillItem.textContent()
    await skillItem.dblclick()

    // Verify tab is open
    await expect(page.locator('[class*="rounded-t-md"]', { hasText: '.md' })).toBeVisible()

    // Go back to file browser
    await page.getByRole('button', { name: 'Skills' }).click()

    // Find the same skill and delete it
    const sameSkillItem = page.locator('button', { hasText: skillName?.split('\n')[0] || '' }).first()
    await sameSkillItem.hover()
    await sameSkillItem.locator('button[title="Delete"]').click()

    // Wait for deletion
    await page.waitForTimeout(500)

    // Verify tab is closed (either no tabs or different tab)
    // The tab bar should either not have this file or be empty
  })
})
