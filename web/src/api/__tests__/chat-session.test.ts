import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  addChatMessage,
  archiveChatSession,
  createChatSession,
  deleteChatSession,
  executeChatSession,
  getChatSession,
  listChatSessionSummaries,
  listChatSessions,
  listChatSessionsByAgent,
  listChatSessionsBySkill,
  rebuildExternalChatSession,
  renameChatSession,
  sendChatMessage,
  subscribeSessionEvents,
  updateChatSession,
} from '@/api/chat-session'
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

async function flushPromises(turns = 4): Promise<void> {
  for (let index = 0; index < turns; index += 1) {
    await Promise.resolve()
  }
}

describe('chat session API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('creates session and maps optional fields to null', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ id: 'session-1' })

    await createChatSession({
      agentId: 'agent-1',
      model: 'gpt-5',
    })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'CreateSession',
      data: {
        agent_id: 'agent-1',
        model: 'gpt-5',
        name: null,
        skill_id: null,
      },
    })
  })

  it('updates session and normalizes undefined fields to null', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ id: 'session-1' })

    await updateChatSession('session-1', { name: 'renamed' })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'UpdateSession',
      data: {
        id: 'session-1',
        updates: {
          agentId: null,
          model: null,
          name: 'renamed',
        },
      },
    })
  })

  it('forwards CRUD and messaging requests', async () => {
    vi.mocked(requestTyped)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce({ deleted: true })
      .mockResolvedValueOnce({ archived: true })
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce({ id: 'session-1' })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ id: 'session-1' })

    await listChatSessions()
    await listChatSessionSummaries()
    await getChatSession('session-1')
    await renameChatSession('session-1', 'new name')
    await deleteChatSession('session-1')
    await archiveChatSession('session-1')
    await rebuildExternalChatSession('session-1')
    await addChatMessage('session-1', { role: 'user', content: 'hi' } as any)
    await sendChatMessage('session-1', 'hello')
    await listChatSessionsByAgent('agent-1')
    await listChatSessionsBySkill('skill-1')
    await executeChatSession('session-1')

    expect(requestTyped).toHaveBeenCalledWith({ type: 'ListFullSessions' })
    expect(requestTyped).toHaveBeenCalledWith({ type: 'ListSessions' })
    expect(requestTyped).toHaveBeenCalledWith({ type: 'GetSession', data: { id: 'session-1' } })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'RenameSession',
      data: { id: 'session-1', name: 'new name' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'DeleteSession',
      data: { id: 'session-1' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ArchiveSession',
      data: { id: 'session-1' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'RebuildExternalSession',
      data: { id: 'session-1' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'AppendMessage',
      data: {
        session_id: 'session-1',
        message: expect.objectContaining({ role: 'user' }),
      },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'AddMessage',
      data: { session_id: 'session-1', role: 'user', content: 'hello' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListSessionsByAgent',
      data: { agent_id: 'agent-1' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListSessionsBySkill',
      data: { skill_id: 'skill-1' },
    })
    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ExecuteChatSession',
      data: { session_id: 'session-1', user_input: null },
    })
  })

  it('subscribes to daemon session events over the shared stream endpoint', async () => {
    const callback = vi.fn()
    vi.mocked(streamClient).mockReturnValue(
      createFrames([
        {
          stream_type: 'event',
          data: { event: { session: { type: 'Updated', session_id: 'session-1' } } },
        } as StreamFrame,
        { stream_type: 'done', data: { total_tokens: null } } as StreamFrame,
      ]),
    )

    const unlisten = await subscribeSessionEvents(callback)
    await flushPromises()

    expect(streamClient).toHaveBeenCalledWith(
      { type: 'SubscribeSessionEvents' },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    )
    expect(callback).toHaveBeenCalledWith({ type: 'Updated', session_id: 'session-1' })

    unlisten()
  })

  it('propagates request errors', async () => {
    vi.mocked(requestTyped).mockRejectedValue(new Error('session not found'))

    await expect(getChatSession('missing')).rejects.toThrow('session not found')
  })
})
