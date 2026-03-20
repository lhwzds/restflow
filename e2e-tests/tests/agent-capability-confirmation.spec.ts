import { expect, test, type Locator, type Page } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type SessionSummary = {
  id: string
  updated_at: number
}

function confirmationError(token: string, message: string) {
  return {
    code: 428,
    kind: 'confirmation_required',
    message: 'Confirmation required',
    details: {
      assessment: {
        operation: 'test',
        intent: 'save',
        status: 'warning',
        effective_model_ref: null,
        warnings: [
          {
            code: 'provider_unavailable',
            message,
          },
        ],
        blockers: [],
        requires_confirmation: true,
        confirmation_token: token,
      },
    },
  }
}

async function openSessionMenu(page: Page, sessionRow: Locator) {
  const menuTrigger = sessionRow.locator('button').last()

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
  test('shows confirmation dialog when create agent needs provider confirmation', async ({ page }) => {
    await page.route('**/api/request', async (route) => {
      const payload = route.request().postDataJSON()
      if (payload?.type !== 'CreateAgent') {
        await route.continue()
        return
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          response_type: 'Error',
          data: confirmationError('token-create-1', 'Provider is not configured.'),
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

    const confirmDialog = page.getByRole('alertdialog')
    await expect(confirmDialog).toBeVisible()
    await expect(confirmDialog).toContainText('Provider is not configured.')
    await expect(confirmDialog.getByRole('button', { name: 'Create anyway' })).toBeVisible()
  })

  test('shows confirmation dialog when session conversion needs provider confirmation', async ({
    page,
  }) => {
    await page.route('**/api/background-agents/convert-session', async (route) => {
      await route.fulfill({
        status: 428,
        contentType: 'application/json',
        body: JSON.stringify(
          confirmationError('token-convert-1', 'Background agent provider needs confirmation.'),
        ),
      })
    })

    await goToWorkspace(page)
    await page.getByRole('button', { name: 'New Session' }).click()

    const sessions = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
    const sessionId = [...sessions].sort((left, right) => right.updated_at - left.updated_at)[0]?.id
    if (!sessionId) {
      throw new Error('Failed to locate the newly created workspace session')
    }

    const sessionRow = page.getByTestId(`session-row-${sessionId}`)
    await expect(sessionRow).toBeVisible()

    const convertItem = await openSessionMenu(page, sessionRow)
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

    const confirmDialog = page.getByRole('alertdialog')
    await expect(confirmDialog).toBeVisible()
    await expect(confirmDialog).toContainText('Background agent provider needs confirmation.')
    await expect(confirmDialog.getByRole('button', { name: 'Create anyway' })).toBeVisible()
  })
})
