import { expect, test } from '@playwright/test'
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

    await sessionRow.locator('button').click()
    await page.getByRole('menuitem', { name: 'Convert to Background Agent' }).click()

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
