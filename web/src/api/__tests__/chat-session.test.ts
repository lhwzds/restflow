import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invokeCommand } from '../tauri-client'
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

const listenMock = vi.fn()

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  invokeCommand: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)

describe('Chat Session API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listenMock.mockReset()
  })

  it('creates session and maps optional fields to null', async () => {
    mockedInvokeCommand.mockResolvedValue({ id: 'session-1' })

    await createChatSession({
      agentId: 'agent-1',
      model: 'gpt-5',
    })

    expect(mockedInvokeCommand).toHaveBeenCalledWith(
      'createChatSession',
      'agent-1',
      'gpt-5',
      null,
      null,
    )
  })

  it('updates session and normalizes undefined fields to null', async () => {
    mockedInvokeCommand.mockResolvedValue({ id: 'session-1' })

    await updateChatSession('session-1', { name: 'renamed' })

    expect(mockedInvokeCommand).toHaveBeenCalledWith('updateChatSession', 'session-1', {
      agentId: null,
      model: null,
      name: 'renamed',
    })
  })

  it('forwards all CRUD and messaging commands', async () => {
    mockedInvokeCommand.mockResolvedValue(true)

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

    expect(mockedInvokeCommand).toHaveBeenCalledWith('listChatSessions')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('listChatSessionSummaries')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('getChatSession', 'session-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('renameChatSession', 'session-1', 'new name')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('deleteChatSession', 'session-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('archiveChatSession', 'session-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('rebuildExternalChatSession', 'session-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith(
      'addChatMessage',
      'session-1',
      expect.objectContaining({ role: 'user' }),
    )
    expect(mockedInvokeCommand).toHaveBeenCalledWith('sendChatMessage', 'session-1', 'hello')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('listChatSessionsByAgent', 'agent-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('listChatSessionsBySkill', 'skill-1')
    expect(mockedInvokeCommand).toHaveBeenCalledWith('executeChatSession', 'session-1')
  })

  it('subscribes with dynamic event name and forwards payload', async () => {
    const callback = vi.fn()
    const unlisten = vi.fn()

    mockedInvokeCommand.mockResolvedValueOnce('session-change-event')
    listenMock.mockResolvedValue(unlisten)

    const result = await subscribeSessionEvents(callback)

    expect(mockedInvokeCommand).toHaveBeenCalledWith('getSessionChangeEventName')
    expect(listenMock).toHaveBeenCalledWith('session-change-event', expect.any(Function))

    const listener = listenMock.mock.calls[0][1] as (event: { payload: unknown }) => void
    listener({ payload: { type: 'Updated', session_id: 'session-1' } })
    expect(callback).toHaveBeenCalledWith({ type: 'Updated', session_id: 'session-1' })
    expect(result).toBe(unlisten)
  })

  it('propagates invoke errors', async () => {
    mockedInvokeCommand.mockRejectedValue(new Error('session not found'))

    await expect(getChatSession('missing')).rejects.toThrow('session not found')
  })
})
