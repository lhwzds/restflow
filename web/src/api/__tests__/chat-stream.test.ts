import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invokeCommand } from '../tauri-client'
import { cancelChatStream, sendChatMessageStream, steerChatStream } from '@/api/chat-stream'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  invokeCommand: vi.fn(),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)

describe('Chat Stream API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('sends streaming chat message with session and content', async () => {
    mockedInvokeCommand.mockResolvedValue('msg-123')

    const result = await sendChatMessageStream('session-1', 'hello')

    expect(mockedInvokeCommand).toHaveBeenCalledWith('sendChatMessageStream', 'session-1', 'hello')
    expect(result).toBe('msg-123')
  })

  it('cancels stream with session and message id', async () => {
    mockedInvokeCommand.mockResolvedValue(undefined)

    await cancelChatStream('session-1', 'msg-123')

    expect(mockedInvokeCommand).toHaveBeenCalledWith('cancelChatStream', 'session-1', 'msg-123')
  })

  it('sends steering instruction for active stream', async () => {
    mockedInvokeCommand.mockResolvedValue(true)

    const result = await steerChatStream('session-1', 'focus on latest error')

    expect(mockedInvokeCommand).toHaveBeenCalledWith(
      'steerChatStream',
      'session-1',
      'focus on latest error',
    )
    expect(result).toBe(true)
  })

  it('propagates invoke errors', async () => {
    mockedInvokeCommand.mockRejectedValue(new Error('stream unavailable'))

    await expect(sendChatMessageStream('session-1', 'test')).rejects.toThrow('stream unavailable')
  })
})
