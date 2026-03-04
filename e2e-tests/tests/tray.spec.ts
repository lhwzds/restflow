import { test, expect } from '@playwright/test'

test.describe('Tray Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      window.localStorage.setItem('locale', 'en')
    })
    await page.goto('/tray')
    await page.waitForLoadState('networkidle')
  })

  test('renders mini dashboard and empty states', async ({ page }) => {
    await expect(page.getByTestId('tray-dashboard-root')).toBeVisible()
    await expect(page.getByText('Mini Dashboard')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'CLI Daemon' })).toBeVisible()
    await expect(page.getByTestId('tray-daemon-status')).toContainText('Running')
    await expect(page.getByTestId('tray-kpi-running')).toBeVisible()
    await expect(page.getByText('No background agents found.')).toBeVisible()
    await expect(page.getByText('No model usage data yet.')).toBeVisible()
  })

  test('controls cli daemon lifecycle from tray', async ({ page }) => {
    const daemonStatus = page.getByTestId('tray-daemon-status')
    await expect(daemonStatus).toContainText('Running')

    await page.getByTestId('tray-daemon-stop').click()
    await expect(daemonStatus).toContainText('Not Running')

    await page.getByTestId('tray-daemon-start').click()
    await expect(daemonStatus).toContainText('Running')

    await page.getByTestId('tray-daemon-restart').click()
    await expect(daemonStatus).toContainText('Running')
  })
})
