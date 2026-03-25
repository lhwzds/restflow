import { test, expect } from '@playwright/test'
import {
  cleanupTrackedState,
  createSessionForTest,
  goToWorkspace,
  requestIpc,
} from './helpers'

test.describe('Chat Voice Transcript', () => {
  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('persists structured transcript metadata into chat history', async ({ page }) => {
    await goToWorkspace(page)
    const sessionId = await createSessionForTest(page)
    type ChatSession = {
      messages?: Array<{
        id: string
        content?: string
        transcript?: { text?: string }
      }>
    }

    const transcriptText = 'e2e structured transcript'
    const now = Date.now()
    const messageId = `e2e-voice-transcript-${now}`

    await requestIpc<ChatSession>(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: messageId,
          role: 'user',
          content:
            `[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/e2e-voice.webm\n\n[Transcript]\n${transcriptText}`,
          timestamp: now,
          execution: null,
          media: {
            media_type: 'voice',
            file_path: '/tmp/e2e-voice.webm',
            duration_sec: 3,
          },
          transcript: {
            text: transcriptText,
            model: 'whisper-1',
            updated_at: now,
          },
        },
      },
    })

    const persisted = await requestIpc<ChatSession>(page, {
      type: 'GetSession',
      data: { id: sessionId },
    })
    const persistedMessage = persisted.messages?.find((message) => message.id === messageId)
    expect(persistedMessage?.transcript?.text).toBe(transcriptText)
    expect(persistedMessage?.content ?? '').toContain(transcriptText)

    await page.reload()
    await page.waitForLoadState('domcontentloaded')
    await expect(page.getByRole('button', { name: 'New Session' })).toBeVisible()
    await page.goto(`/workspace/c/${sessionId}`)
    await page.waitForLoadState('domcontentloaded')
    await expect(page.getByText(transcriptText)).toBeVisible()
  })
})
