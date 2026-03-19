import { expect, test, type Locator, type Page } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type SessionSummary = {
  id: string
  updated_at: number
}

type BackgroundAgentSummary = {
  id: string
  chat_session_id?: string | null
}

test.describe('Background Agent Web Flow', () => {
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

  test('converts a workspace session into a background agent from the web UI', async ({ page }) => {
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
    await expect(convertItem).toBeVisible()
    await convertItem.click()

    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    const nameInput = dialog.locator('input').first()
    await expect(nameInput).toBeVisible()
    await nameInput.fill(`E2E Background ${Date.now()}`)
    await dialog.locator('textarea').fill('Convert this session into a background agent')

    const convertButton = dialog.getByRole('button', { name: 'Convert' })
    await expect(convertButton).toBeEnabled()
    await convertButton.click()
    await expect(dialog).not.toBeVisible()

    await expect(sessionRow.getByText('background')).toBeVisible()

    await expect
      .poll(async () => {
        const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
          type: 'ListBackgroundAgents',
          data: { status: null },
        })

        return agents.some((agent) => agent.chat_session_id === sessionId)
      })
      .toBe(true)
  })
})
