import { test, expect } from '@playwright/test'
import { goToWorkspace } from './helpers'

test.describe('Chat Voice Transcript', () => {
  test('renders structured transcript from chat history metadata', async ({ page }) => {
    await goToWorkspace(page)

    await page.evaluate(async () => {
      const invoke = (window as any).__TAURI_INTERNALS__?.invoke as
        | ((cmd: string, args?: Record<string, unknown>) => Promise<any>)
        | undefined
      if (!invoke) {
        throw new Error('Tauri invoke is not available')
      }

      const summaries = await invoke('list_chat_session_summaries')
      const sessionId = summaries?.[0]?.id as string | undefined
      if (!sessionId) {
        throw new Error('No chat session available for e2e test')
      }

      await invoke('add_chat_message', {
        sessionId,
        message: {
          id: 'e2e-voice-transcript-message',
          role: 'user',
          content: '[Voice message]',
          timestamp: BigInt(Date.now()),
          execution: null,
          media: {
            media_type: 'voice',
            file_path: '/tmp/e2e-voice.webm',
            duration_sec: 3,
          },
          transcript: {
            text: 'e2e structured transcript',
            model: 'whisper-1',
            updated_at: Date.now(),
          },
        },
      })
    })

    await page.reload()
    await page.waitForLoadState('networkidle')

    await expect(page.getByText('e2e structured transcript')).toBeVisible()
  })
})
