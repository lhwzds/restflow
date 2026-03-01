import { test, expect } from '@playwright/test'
import { goToWorkspace } from './helpers'

test.describe('Chat Voice Transcript', () => {
  test('persists structured transcript metadata into chat history', async ({ page }) => {
    await goToWorkspace(page)
    await page.getByRole('button', { name: 'New Session' }).click()

    const payload = await page.evaluate(async () => {
      const invoke = (window as any).__TAURI_INTERNALS__?.invoke as
        | ((cmd: string, args?: Record<string, unknown>) => Promise<any>)
        | undefined
      if (!invoke) {
        throw new Error('Tauri invoke is not available')
      }

      const transcriptText = 'e2e structured transcript'
      const now = Date.now()
      const messageId = `e2e-voice-transcript-${now}`

      const summaries = await invoke('list_chat_session_summaries')
      const sessionId = summaries?.[0]?.id as string | undefined
      if (!sessionId) {
        throw new Error('Failed to locate latest chat session for e2e test')
      }

      await invoke('add_chat_message', {
        sessionId,
        message: {
          id: messageId,
          role: 'user',
          content: `[Voice message]\n\n[Transcript]\n${transcriptText}`,
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
      })

      const session = await invoke('get_chat_session', { id: sessionId })
      const persistedMessage = Array.isArray(session?.messages)
        ? session.messages.find((msg: any) => msg.id === messageId)
        : null
      if (!persistedMessage) {
        throw new Error('Injected voice message was not persisted')
      }

      return {
        transcriptText,
        persistedTranscript: persistedMessage.transcript?.text ?? null,
        persistedContent: persistedMessage.content ?? '',
      }
    })

    await page.reload()
    await page.waitForLoadState('networkidle')
    expect(payload.persistedTranscript).toBe(payload.transcriptText)
    expect(payload.persistedContent).toContain(payload.transcriptText)
  })
})
