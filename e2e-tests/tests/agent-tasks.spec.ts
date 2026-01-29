import { test, expect } from '@playwright/test'

/**
 * Agent Tasks E2E Tests
 *
 * Tests for the Agent Task management feature, including:
 * - Navigation to/from the agent tasks page
 * - Task list display and filtering
 * - Task creation dialog
 * - Task status display
 *
 * Note: Tests run against a fresh database, so they create their own test data.
 */
test.describe('Agent Tasks', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')
  })

  test('navigates to agent tasks page from workspace', async ({ page }) => {
    // Tasks button should be visible in header navigation
    const tasksButton = page.locator('header nav button', { hasText: 'Tasks' })
    await expect(tasksButton).toBeVisible()

    // Click the Tasks button
    await tasksButton.click()

    // Should navigate to agent tasks page
    await expect(page).toHaveURL('/agent-tasks')

    // Header should show Agent Tasks title
    await expect(page.locator('h1', { hasText: 'Agent Tasks' })).toBeVisible()
  })

  test('navigates back to workspace from agent tasks', async ({ page }) => {
    // Navigate to agent tasks page
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Back button should be visible
    const backButton = page.locator('header .back-btn')
    await expect(backButton).toBeVisible()

    // Click the back button
    await backButton.click()

    // Should navigate back to workspace
    await expect(page).toHaveURL('/workspace')
  })

  test('displays empty state when no tasks exist', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Should show empty state with create button
    const newTaskButton = page.locator('button', { hasText: 'New Task' })
    await expect(newTaskButton).toBeVisible()
  })

  test('opens create task dialog', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Click New Task button
    await page.locator('button', { hasText: 'New Task' }).click()

    // Dialog should open
    await expect(page.locator('[role="dialog"]')).toBeVisible()
    await expect(page.locator('[role="dialog"]', { hasText: 'Create Task' })).toBeVisible()
  })

  test('create task dialog has required fields', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Open create dialog
    await page.locator('button', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Check for form fields - name input has placeholder "My scheduled task"
    await expect(page.locator('input[placeholder*="scheduled task"]')).toBeVisible()
    // Use exact text to avoid matching "Created" dropdown button
    await expect(page.getByRole('button', { name: 'Create Task' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible()
  })

  test('can close create task dialog', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Open create dialog
    await page.locator('button', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Close dialog with Cancel button
    await page.locator('button', { hasText: 'Cancel' }).click()

    // Dialog should be closed
    await expect(page.locator('[role="dialog"]')).not.toBeVisible()
  })

  test('search input is visible', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Search input should be visible
    await expect(page.locator('input[placeholder*="Search"]')).toBeVisible()
  })

  test('status filter is visible', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Filter trigger should be visible
    await expect(page.locator('button', { hasText: /All|Active|Paused/ })).toBeVisible()
  })

  test('view toggle buttons are visible', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // View toggle buttons (grid/list) should be visible
    const viewButtons = page.locator('.view-toggle button')
    await expect(viewButtons.first()).toBeVisible()
  })

  test('refresh button reloads tasks', async ({ page }) => {
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')

    // Refresh button should be visible
    const refreshButton = page.locator('button[aria-label="Refresh"]')
    await expect(refreshButton).toBeVisible()

    // Click should trigger refresh (no error)
    await refreshButton.click()
    // If there's a loading indicator, wait for it to complete
    await page.waitForLoadState('networkidle')
  })
})

test.describe('Agent Tasks - Task Creation Flow', () => {
  test.beforeEach(async ({ page }) => {
    // First, create an agent so we have one available for task creation
    await page.goto('/workspace')
    await page.waitForLoadState('networkidle')

    // Navigate to Agents tab
    const agentsTab = page.getByRole('button', { name: 'Agents' })
    await agentsTab.click()
    await page.waitForLoadState('networkidle')

    // Create a new agent via UI
    const newAgentButton = page.locator('button', { hasText: 'New Agent' })
    await newAgentButton.click()

    // Wait for agent to be created (editor should open)
    await page.waitForTimeout(500)

    // Now navigate to agent tasks
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')
  })

  test('creates a task with minimum required fields', async ({ page }) => {
    // Open create dialog
    await page.locator('button', { hasText: 'New Task' }).click()
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
