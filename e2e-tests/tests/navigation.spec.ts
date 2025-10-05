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
    const collapseBtn = page.locator('.collapse-btn')
    const expandBtn = page.locator('.expand-btn')

    // Verify sidebar starts expanded
    await expect(collapseBtn).toBeVisible()

    // Click collapse button
    await collapseBtn.click()

    // Wait for expand button to appear and collapse button to disappear
    await expect(expandBtn).toBeVisible()
    await expect(collapseBtn).not.toBeVisible()

    // Verify sidebar is now narrow (collapsed)
    await expect(sidePanel).toHaveCSS('width', /^(6[0-9]|[1-9][0-9])px$/) // ~64px or similar

    // Click expand button
    await expandBtn.click()

    // Wait for collapse button to reappear and expand button to disappear
    await expect(collapseBtn).toBeVisible()
    await expect(expandBtn).not.toBeVisible()

    // Verify sidebar is now wide (expanded)
    await expect(sidePanel).toHaveCSS('width', /(1[5-9][0-9]|2[0-9]{2})px/) // ~200px or similar
  })
})
