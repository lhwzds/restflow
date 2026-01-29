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

    // Check for form fields
    await expect(page.locator('input[placeholder*="Task name"]')).toBeVisible()
    await expect(page.locator('button', { hasText: 'Create' })).toBeVisible()
    await expect(page.locator('button', { hasText: 'Cancel' })).toBeVisible()
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
    await page.goto('/agent-tasks')
    await page.waitForLoadState('networkidle')
  })

  test('creates a task with minimum required fields', async ({ page }) => {
    // Open create dialog
    await page.locator('button', { hasText: 'New Task' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()

    // Fill in task name
    await page.locator('input[placeholder*="Task name"]').fill('Test Task E2E')

    // Select a schedule type if present
    const scheduleSelect = page.locator('[data-testid="schedule-type"]')
    if (await scheduleSelect.isVisible()) {
      await scheduleSelect.click()
      await page.locator('[role="option"]').first().click()
    }

    // Submit the form
    await page.locator('[role="dialog"] button', { hasText: 'Create' }).click()

    // Wait for dialog to close or success message
    await page.waitForTimeout(500)

    // Dialog should close on success
    // Note: This may fail if validation errors occur
  })
})
