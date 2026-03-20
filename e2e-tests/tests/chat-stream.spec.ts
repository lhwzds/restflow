import { test, expect } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type StreamFrame = {
  stream_type: string
  data: {
    event?: {
      session?: {
        type?: string
        session_id?: string
        source?: string
      }
    }
    stream_id?: string
  }
}

type SessionSummary = {
  id: string
  updated_at: number
}

type StreamCollectionOptions = {
  maxFrames: number
  timeoutMs: number
}

async function waitForLatestSessionId(page: Parameters<typeof goToWorkspace>[0]): Promise<string> {
  await expect
    .poll(
      async () => {
        const summaries = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
        return [...summaries].sort((left, right) => right.updated_at - left.updated_at)[0]?.id ?? null
      },
      {
        timeout: 5000,
        message: 'Expected a chat session to be created after clicking New Session',
      },
    )
    .not.toBeNull()

  const summaries = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })
  const sessionId = [...summaries].sort((left, right) => right.updated_at - left.updated_at)[0]?.id
  if (!sessionId) {
    throw new Error('No chat session available for stream event test')
  }

  return sessionId
}

async function collectSessionFrames(
  page: Parameters<typeof goToWorkspace>[0],
  options: Partial<StreamCollectionOptions> = {},
) {
  return page.evaluate(async ({ maxFrames, timeoutMs }) => {
    const controller = new AbortController()
    const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs)
    const response = await fetch('/api/stream', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/x-ndjson',
      },
      body: JSON.stringify({ type: 'SubscribeSessionEvents' }),
      signal: controller.signal,
    })

    if (!response.ok || !response.body) {
      window.clearTimeout(timeoutId)
      throw new Error(`Failed to open session event stream: HTTP ${response.status}`)
    }

    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    const frames: Array<{ stream_type: string; data: unknown }> = []
    let buffer = ''

    try {
      while (frames.length < maxFrames) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split('\n')
        buffer = lines.pop() ?? ''

        for (const line of lines) {
          const trimmed = line.trim()
          if (!trimmed) continue
          frames.push(JSON.parse(trimmed))
          if (frames.length >= maxFrames) {
            break
          }
        }
      }
    } catch (error) {
      if (!(error instanceof DOMException && error.name === 'AbortError')) {
        throw error
      }
    } finally {
      window.clearTimeout(timeoutId)
      await reader.cancel().catch(() => undefined)
    }

    return frames
  }, { maxFrames: options.maxFrames ?? 8, timeoutMs: options.timeoutMs ?? 4000 }) as Promise<
    StreamFrame[]
  >
}

function findEventFrame(
  frames: StreamFrame[],
  eventType: string,
  sessionId?: string,
): StreamFrame | undefined {
  return frames.find(
    (frame) =>
      frame.stream_type === 'Event' &&
      frame.data.event?.session?.type === eventType &&
      (sessionId === undefined || frame.data.event?.session?.session_id === sessionId),
  )
}

test.describe('Chat streaming', () => {
  test.beforeEach(async ({ page }) => {
    await goToWorkspace(page)
  })

  test('receives created session events from the daemon stream endpoint', async ({ page }) => {
    const streamPromise = collectSessionFrames(page)

    await page.getByRole('button', { name: 'New Session' }).click()

    const frames = await streamPromise
    expect(frames.some((frame) => frame.stream_type === 'Start')).toBe(true)
    const createdFrame = findEventFrame(frames, 'Created')
    expect(createdFrame, `Expected a Created event, received ${JSON.stringify(frames)}`).toBeDefined()
  })

  test('receives message-added events from the daemon stream endpoint', async ({ page }) => {
    await page.getByRole('button', { name: 'New Session' }).click()
    const sessionId = await waitForLatestSessionId(page)

    const streamPromise = collectSessionFrames(page)

    await requestIpc(page, {
      type: 'AppendMessage',
      data: {
        session_id: sessionId,
        message: {
          id: `e2e-stream-message-${Date.now()}`,
          role: 'user',
          content: 'stream event payload',
          timestamp: Date.now(),
        },
      },
    })

    const frames = await streamPromise
    expect(frames.some((frame) => frame.stream_type === 'Start')).toBe(true)
    const messageAddedFrame = findEventFrame(frames, 'MessageAdded', sessionId)
    expect(
      messageAddedFrame,
      `Expected a MessageAdded event for session ${sessionId}, received ${JSON.stringify(frames)}`,
    ).toBeDefined()
  })
})
