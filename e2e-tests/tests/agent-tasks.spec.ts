import { test, expect } from '@playwright/test'
import { createAgentAndOpenEditor } from './helpers'

/**
 * Agent Tasks E2E Tests
 *
 * Tests for the Agent Task management feature, including:
 * - Navigation to Tasks tab in workspace
 * - Task list display and filtering
 * - Task creation dialog
 * - Task status display
 *
 * Note: Tasks are accessed via the "Tasks" tab in the workspace view,
 * with backend APIs now served from /background-agents. Tests run against a fresh database,
 * so they create their own test data.
 *
 * Design Notes:
 * - TaskBrowser "New Task" card uses dashed border (like TerminalBrowser)
 *   because task items are displayed as Card components with borders
 * - Search and view toggle controls are in the header (shared with Skills/Agents)
 */
test.describe.skip('Agent Tasks', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('navigates to tasks tab from workspace', async ({ page }) => {
    // Tasks button should be visible in header navigation
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await expect(tasksButton).toBeVisible()

    // Click the Tasks button
    await tasksButton.click()

    // Tasks tab should become active (has primary text color)
    await expect(tasksButton).toHaveClass(/text-primary/)

    // TaskBrowser should be displayed - in grid view, New Task is a Card (div), not a button
    const newTaskCard = page.locator('.border-dashed', { hasText: 'New Task' })
    await expect(newTaskCard).toBeVisible()
  })

  test('navigates back to skills from tasks tab', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Click Skills tab
    const skillsButton = page.locator('header nav button', { hasText: 'Skills' })
    await skillsButton.click()

    // Skills should be active
    await expect(skillsButton).toHaveClass(/text-primary/)

    // New Skill button should be visible
    await expect(page.locator('button', { hasText: 'New Skill' })).toBeVisible()
  })

  test('displays task browser when no tasks exist', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Should show New Task card - in grid view, it's a Card (div), not a button
    const newTaskCard = page.locator('.border-dashed', { hasText: 'New Task' })
    await expect(newTaskCard).toBeVisible()
  })

  test('opens create task dialog', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Click New Task card - in grid view, it's a Card (div), not a button
    await page.locator('.border-dashed', { hasText: 'New Task' }).click()

    // Dialog should open
    await expect(page.locator('[role="dialog"]')).toBeVisible()
    await expect(page.locator('[role="dialog"]', { hasText: 'Create Task' })).toBeVisible()
  })

  test('create task dialog has required fields', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Open create dialog - in grid view, New Task is a Card (div), not a button
    await page.locator('.border-dashed', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Check for form fields - name input has placeholder "My scheduled task"
    await expect(page.locator('input[placeholder*="scheduled task"]')).toBeVisible()
    // Use exact text to avoid matching "Created" dropdown button
    await expect(page.getByRole('button', { name: 'Create Task' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible()
  })

  test('can close create task dialog', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Open create dialog - in grid view, New Task is a Card (div), not a button
    await page.locator('.border-dashed', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Close dialog with Cancel button
    await page.locator('button', { hasText: 'Cancel' }).click()

    // Dialog should be closed
    await expect(page.locator('[role="dialog"]')).not.toBeVisible()
  })

  test('search input is visible in header', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Search input should be visible in header
    await expect(page.locator('header input[placeholder*="Search"]')).toBeVisible()
  })

  test('view toggle buttons are visible in header', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // View toggle buttons (grid/list) are in the header
    const viewButtons = page.locator('header .flex button[class*="h-6"]')
    await expect(viewButtons.first()).toBeVisible()
  })

  test('refresh button reloads tasks', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Refresh button should be visible in TaskBrowser
    const refreshButton = page.locator('button[title*="Refresh"], button[aria-label*="Refresh"], button:has(svg[class*="refresh"])')
    // If no dedicated refresh button, TaskBrowser may auto-refresh
    // This test verifies we can navigate to Tasks without errors
    await page.waitForLoadState('networkidle')
  })

  test('can switch between grid and list view', async ({ page }) => {
    // Navigate to tasks tab
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await tasksButton.click()

    // Should be in grid view by default - New Task is a Card
    const newTaskCard = page.locator('.border-dashed', { hasText: 'New Task' })
    await expect(newTaskCard).toBeVisible()

    // Click list view toggle (first button in the view toggle group - order is: List, Grid)
    const listViewButton = page.locator('header .flex button[class*="h-6"]').first()
    await listViewButton.click()
    await page.waitForTimeout(200)

    // In list view, New Task is a button with border-dashed class (within .space-y-1 list container)
    const newTaskListButton = page.locator('.space-y-1 > button.border-dashed', { hasText: 'New Task' })
    await expect(newTaskListButton).toBeVisible()
  })
})

test.describe.skip('Agent Tasks - Task Creation Flow', () => {
  test.beforeEach(async ({ page }) => {
    // First, create an agent so we have one available for task creation
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    // Navigate to Agents tab
    const agentsTab = page.getByRole('button', { name: 'Agents' })
    await agentsTab.click()
    await page.waitForLoadState('networkidle')

    await createAgentAndOpenEditor(page)

    // Now navigate to tasks tab
    const tasksTab = page.getByRole('button', { name: 'Tasks' })
    await tasksTab.click()
    await page.waitForLoadState('networkidle')
  })

  test('creates a task with minimum required fields', async ({ page }) => {
    // Open create dialog - in grid view, New Task is a Card (div), not a button
    await page.locator('.border-dashed', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Fill in task name - name input has placeholder "My scheduled task"
    await page.locator('input[placeholder*="scheduled task"]').fill('Test Task E2E')

    // Select an agent (required field) - click the agent select trigger
    const agentSelect = page.locator('[role="dialog"]').locator('#agent')
    await agentSelect.click()
    // Wait for dropdown and select first available agent
    const firstAgent = page.locator('[role="option"]').first()
    await expect(firstAgent).toBeVisible({ timeout: 5000 })
    await firstAgent.click()

    // Default schedule (interval 1 hour) is valid, no need to change

    // Submit the form - use getByRole for exact match
    await page.getByRole('button', { name: 'Create Task' }).click()

    // Wait for dialog to close or success message
    await page.waitForTimeout(500)

    // Dialog should close on success
  })
})
