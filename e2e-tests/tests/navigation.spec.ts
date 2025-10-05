import { test, expect } from '@playwright/test'

/**
 * Sidebar Navigation E2E Tests
 */
test.describe('Sidebar Navigation', () => {
  test('clicking Agents menu should open Agents page', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')

    // Click Agents menu
    await page.click('text=Agents')

    // Verify URL
    await expect(page).toHaveURL('/agents')

    // Verify menu is highlighted
    const agentsMenuItem = page.locator('.el-menu-item', { hasText: 'Agents' })
    await expect(agentsMenuItem).toHaveClass(/is-active/)
  })

  test('clicking Secrets menu should open Secrets page', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')

    await page.click('text=Secrets')

    await expect(page).toHaveURL('/secrets')

    const secretsMenuItem = page.locator('.el-menu-item', { hasText: 'Secrets' })
    await expect(secretsMenuItem).toHaveClass(/is-active/)
  })

  test('clicking Workflows menu should return to Workflows page', async ({ page }) => {
    await page.goto('/agents')
    await page.waitForLoadState('networkidle')

    await page.click('text=Workflows')

    await expect(page).toHaveURL('/workflows')

    const workflowsMenuItem = page.locator('.el-menu-item', { hasText: 'Workflows' })
    await expect(workflowsMenuItem).toHaveClass(/is-active/)
  })

  test('sidebar can be collapsed and expanded', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')

    const sidePanel = page.locator('.side-panel')

    // Get initial width
    const initialWidth = await sidePanel.evaluate(el => getComputedStyle(el).width)

    // Click collapse button and wait for collapse to complete
    const collapseBtn = page.locator('.collapse-btn')
    await collapseBtn.click()

    // Wait for expand button to appear (indicates collapse is complete)
    const expandBtn = page.locator('.expand-btn')
    await expect(expandBtn).toBeVisible()

    // Verify collapsed state
    const collapsedWidth = await sidePanel.evaluate(el => getComputedStyle(el).width)
    expect(parseInt(collapsedWidth)).toBeLessThan(parseInt(initialWidth))

    // Click expand button and wait for expand to complete
    await expandBtn.click()

    // Wait for collapse button to reappear (indicates expand is complete)
    await expect(collapseBtn).toBeVisible()

    // Verify expanded state
    const expandedWidth = await sidePanel.evaluate(el => getComputedStyle(el).width)
    expect(parseInt(expandedWidth)).toBeGreaterThan(parseInt(collapsedWidth))
  })
})
