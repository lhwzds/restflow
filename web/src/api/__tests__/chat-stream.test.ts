import { beforeEach, describe, expect, it, vi } from 'vitest'
import { cancelChatStream, openChatStream, steerChatStream } from '@/api/chat-stream'
import { requestTyped, streamClient } from '../http-client'
import type { StreamFrame } from '@/types/generated/StreamFrame'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
  streamClient: vi.fn(),
}))

async function* createFrames(frames: StreamFrame[]): AsyncGenerator<StreamFrame> {
  for (const frame of frames) {
    yield frame
  }
}

describe('chat stream API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('crypto', { randomUUID: () => 'stream-123' })
  })

  it('opens a chat stream using daemon stream contracts', async () => {
    const frames = createFrames([{ stream_type: 'start', data: { stream_id: 'stream-123' } }])
    vi.mocked(streamClient).mockReturnValue(frames)

    const handle = openChatStream('session-1', 'hello')
    const first = await handle.frames.next()

    expect(handle.streamId).toBe('stream-123')
    expect(streamClient).toHaveBeenCalledWith(
      {
        type: 'ExecuteChatSessionStream',
        data: {
          session_id: 'session-1',
          user_input: 'hello',
          stream_id: 'stream-123',
        },
      },
      { signal: undefined },
    )
    expect(first.value).toEqual({ stream_type: 'start', data: { stream_id: 'stream-123' } })
  })

  it('cancels an active stream by stream id', async () => {
    vi.mocked(requestTyped).mockResolvedValue(null)

    await cancelChatStream('stream-123')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'CancelChatSessionStream',
      data: { stream_id: 'stream-123' },
    })
  })

  it('sends steering instructions through request contracts', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ steered: true })

    const result = await steerChatStream('session-1', 'focus on latest error')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'SteerChatSessionStream',
      data: {
        session_id: 'session-1',
        instruction: 'focus on latest error',
      },
    })
    expect(result).toBe(true)
  })
})
