import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useChatSessionStore } from '../chatSessionStore'
import * as chatSessionApi from '@/api/chat-session'

vi.mock('@/api/chat-session', () => ({
  listChatSessionSummaries: vi.fn(),
  getChatSession: vi.fn(),
  createChatSession: vi.fn(),
  deleteChatSession: vi.fn(),
  renameChatSession: vi.fn(),
  updateChatSession: vi.fn(),
  sendChatMessage: vi.fn(),
  addChatMessage: vi.fn(),
  executeChatSession: vi.fn(),
  listChatSessionsByAgent: vi.fn(),
  listChatSessionsBySkill: vi.fn(),
}))

describe('chatSessionStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  describe('BigInt sorting', () => {
    it('sorts sessions correctly with BigInt updated_at values', () => {
      const store = useChatSessionStore()

      // Use BigInt values that would overflow Number precision
      store.summaries = [
        {
          id: 'a',
          name: 'Session A',
          agent_id: 'agent-1',
          model: 'gpt-4',
          skill_id: null,
          message_count: 1,
          updated_at: 1700000000000n,
          last_message_preview: null,
        },
        {
          id: 'b',
          name: 'Session B',
          agent_id: 'agent-1',
          model: 'gpt-4',
          skill_id: null,
          message_count: 1,
          updated_at: 1700000000002n,
          last_message_preview: null,
        },
        {
          id: 'c',
          name: 'Session C',
          agent_id: 'agent-1',
          model: 'gpt-4',
          skill_id: null,
          message_count: 1,
          updated_at: 1700000000001n,
          last_message_preview: null,
        },
      ]

      store.sortField = 'updated_at'
      store.sortOrder = 'desc'
      const descResult = store.filteredSummaries
      expect(descResult.map((s) => s.id)).toEqual(['b', 'c', 'a'])

      store.sortOrder = 'asc'
      const ascResult = store.filteredSummaries
      expect(ascResult.map((s) => s.id)).toEqual(['a', 'c', 'b'])
    })

    it('handles BigInt values that exceed Number.MAX_SAFE_INTEGER', () => {
      const store = useChatSessionStore()

      const large1 = BigInt(Number.MAX_SAFE_INTEGER) + 1n
      const large2 = BigInt(Number.MAX_SAFE_INTEGER) + 2n

      store.summaries = [
        {
          id: 'x',
          name: 'X',
          agent_id: 'a',
          model: 'm',
          skill_id: null,
          message_count: 0,
          updated_at: large2,
          last_message_preview: null,
        },
        {
          id: 'y',
          name: 'Y',
          agent_id: 'a',
          model: 'm',
          skill_id: null,
          message_count: 0,
          updated_at: large1,
          last_message_preview: null,
        },
      ]

      store.sortField = 'updated_at'
      store.sortOrder = 'asc'
      expect(store.filteredSummaries.map((s) => s.id)).toEqual(['y', 'x'])
    })
  })

  describe('sendMessageAndExecute race condition', () => {
    it('uses captured sessionId even if currentSessionId changes during await', async () => {
      const store = useChatSessionStore()
      store.currentSessionId = 'session-1'

      const sendResult = {
        id: 'session-1',
        name: 'Test',
        agent_id: 'agent-1',
        model: 'gpt-4',
        skill_id: null,
        messages: [
          { id: 'msg-1', role: 'user' as const, content: 'hello', timestamp: 1000n, execution: null },
        ],
        created_at: 1000n,
        updated_at: 1001n,
        summary_message_id: null,
        prompt_tokens: 0n,
        completion_tokens: 0n,
        cost: 0,
        metadata: {},
      }

      const execResult = {
        ...sendResult,
        messages: [
          ...sendResult.messages,
          {
            id: 'msg-2',
            role: 'assistant' as const,
            content: 'world',
            timestamp: 1002n,
            execution: null,
          },
        ],
        updated_at: 1003n,
      }

      // When sendChatMessage is called, simulate user switching sessions
      vi.mocked(chatSessionApi.sendChatMessage).mockImplementation(async () => {
        store.currentSessionId = 'session-2'
        return sendResult
      })
      vi.mocked(chatSessionApi.executeChatSession).mockResolvedValue(execResult)

      await store.sendMessageAndExecute('hello')

      // executeChatSession should have been called with the captured 'session-1',
      // not the new 'session-2'
      expect(chatSessionApi.executeChatSession).toHaveBeenCalledWith('session-1')
    })
  })
})
