import { expect, test, type Locator, type Page } from '@playwright/test'
import {
  cleanupTrackedState,
  createSessionForTest,
  goToWorkspace,
} from './helpers'

function backendError(message: string) {
  return {
    code: 400,
    kind: 'invalid',
    message,
    details: null,
  }
}

async function openSessionMenu(page: Page, sessionRow: Locator) {
  const header = sessionRow.locator(':scope > div').first()
  const menuTrigger = header.locator('button').last()

  for (let attempt = 0; attempt < 3; attempt += 1) {
    await sessionRow.hover()
    await expect(menuTrigger).toBeVisible()
    await menuTrigger.click({ force: true })

    const convertItem = page.getByRole('menuitem', {
      name: 'Convert to Background Agent',
      exact: true,
    })
    if (await convertItem.isVisible().catch(() => false)) {
      return convertItem
    }

    await page.keyboard.press('Escape').catch(() => {})
  }

  throw new Error('Failed to open session context menu for background-agent conversion')
}

test.describe('Agent capability confirmation', () => {
  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('shows an error toast when create agent fails provider validation', async ({ page }) => {
    let createRequestCount = 0
    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON()
      if (payload?.type !== 'CreateAgent') {
        await route.continue()
        return
      }
      createRequestCount += 1
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          response_type: 'Error',
          data: backendError('Provider is not configured.'),
        }),
      })
    })

    await goToWorkspace(page)
    await page.getByRole('button', { name: 'Agents' }).click()
    await page.getByRole('button', { name: 'Create Agent' }).click()

    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await dialog.locator('input').first().fill('Confirmed Agent')
    const createResponse = page.waitForResponse((response) => {
      if (!response.url().includes('/api/request')) {
        return false
      }
      const request = response.request().postDataJSON()
      return request?.type === 'CreateAgent'
    })
    await dialog.getByRole('button', { name: 'Create' }).click()
    await createResponse

    await expect(dialog).toBeVisible()
    await expect(page.getByText('Provider is not configured.')).toBeVisible()
    await expect(page.getByRole('alertdialog')).toHaveCount(0)
    expect(createRequestCount).toBe(1)
  })

  test('shows an error toast when session conversion fails provider validation', async ({
    page,
  }) => {
    let convertRequestCount = 0
    await page.route('**/api/background-agents/convert-session', async (route) => {
      convertRequestCount += 1
      await route.fulfill({
        status: 400,
        contentType: 'application/json',
        body: JSON.stringify(backendError('Background agent provider needs confirmation.')),
      })
    })

    await goToWorkspace(page)
    const sessionId = await createSessionForTest(page)

    const sessionFolder = page.getByTestId(`workspace-folder-${sessionId}`)
    await expect(sessionFolder).toBeVisible()

    const convertItem = await openSessionMenu(page, sessionFolder)
    await convertItem.click()

    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await dialog.locator('input').first().fill('Confirmed Background Agent')
    await dialog.locator('textarea').fill('Convert after confirmation')
    const convertResponse = page.waitForResponse((response) =>
      response.url().includes('/api/background-agents/convert-session'),
    )
    await dialog.getByRole('button', { name: 'Convert' }).click()
    await convertResponse

    await expect(dialog).toBeVisible()
    await expect(page.getByText('Background agent provider needs confirmation.')).toBeVisible()
    await expect(page.getByRole('alertdialog')).toHaveCount(0)
    expect(convertRequestCount).toBe(1)
  })
})
