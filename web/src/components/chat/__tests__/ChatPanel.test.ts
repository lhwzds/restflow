import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h, nextTick, ref } from 'vue'
import ChatPanel from '../ChatPanel.vue'
import { useChatSession } from '@/composables/workspace/useChatSession'
import { useChatStream } from '@/composables/workspace/useChatStream'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useModelsStore } from '@/stores/modelsStore'
import { listAgents } from '@/api/agents'

type SessionLike = {
  id: string
  name: string
  agent_id: string
  model: string
  skill_id: string | null
  messages: any[]
  created_at: bigint
  updated_at: bigint
  summary_message_id: string | null
  prompt_tokens: bigint
  completion_tokens: bigint
  cost: number
  metadata: Record<string, unknown>
}

const mockCurrentSession = ref<SessionLike | null>(null)
const mockMessages = ref<any[]>([])
const mockIsSending = ref(false)

const mockStreamState = ref({
  messageId: null as string | null,
  content: '',
  thinking: '',
  steps: [] as any[],
  isStreaming: false,
  error: null as string | null,
  tokenCount: 0,
  inputTokens: 0,
  outputTokens: 0,
})
const mockIsStreaming = ref(false)
const mockTokensPerSecond = ref(0)
const mockDuration = ref(0)

const mockRefreshSession = vi.fn()
const mockUpdateSessionLocally = vi.fn()
const mockUpdateSessionAgent = vi.fn()
const mockUpdateSessionModel = vi.fn()
const mockCreateSession = vi.fn()
const mockSendStream = vi.fn()
const mockCancelStream = vi.fn()
const mockResetStream = vi.fn()
const mockLoadModels = vi.fn()
const mockModels: Array<{ model: string; name: string }> = []

let chatBoxMountCount = 0

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/components/chat/MessageList.vue', () => ({
  default: {
    name: 'MessageList',
    template: '<div data-testid="message-list" />',
  },
}))

vi.mock('@/components/workspace/ChatBox.vue', () => ({
  default: defineComponent({
    name: 'ChatBox',
    props: {
      selectedModel: {
        type: String,
        default: '',
      },
    },
    setup(props) {
      chatBoxMountCount += 1
      return () =>
        h(
          'div',
          {
            'data-testid': 'chatbox',
            'data-selected-model': props.selectedModel ?? '',
          },
          props.selectedModel ?? '',
        )
    },
  }),
}))

vi.mock('@/composables/workspace/useChatSession', () => ({
  useChatSession: vi.fn(),
}))

vi.mock('@/composables/workspace/useChatStream', () => ({
  useChatStream: vi.fn(),
}))

vi.mock('@/stores/chatSessionStore', () => ({
  useChatSessionStore: vi.fn(),
}))

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: vi.fn(),
}))

vi.mock('@/api/agents', () => ({
  listAgents: vi.fn(),
}))

vi.mock('@/api/chat-session', () => ({
  sendChatMessage: vi.fn(),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  }),
}))

function createSession(model: string): SessionLike {
  return {
    id: 'session-1',
    name: 'Test Session',
    agent_id: 'agent-1',
    model,
    skill_id: null,
    messages: [],
    created_at: 1000n,
    updated_at: 1000n,
    summary_message_id: null,
    prompt_tokens: 0n,
    completion_tokens: 0n,
    cost: 0,
    metadata: {},
  }
}

describe('ChatPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    chatBoxMountCount = 0

    mockCurrentSession.value = createSession('gpt-4')
    mockMessages.value = []
    mockIsSending.value = false

    mockStreamState.value = {
      messageId: null,
      content: '',
      thinking: '',
      steps: [],
      isStreaming: false,
      error: null,
      tokenCount: 0,
      inputTokens: 0,
      outputTokens: 0,
    }
    mockIsStreaming.value = false
    mockTokensPerSecond.value = 0
    mockDuration.value = 0

    mockModels.length = 0
    mockLoadModels.mockImplementation(async () => {
      mockModels.splice(
        0,
        mockModels.length,
        { model: 'gpt-4', name: 'GPT-4' },
        { model: 'gpt-5', name: 'GPT-5' },
      )
    })

    vi.mocked(useChatSession).mockReturnValue({
      currentSession: mockCurrentSession,
      messages: mockMessages,
      isSending: mockIsSending,
      createSession: mockCreateSession,
    } as any)

    vi.mocked(useChatStream).mockReturnValue({
      state: mockStreamState,
      isStreaming: mockIsStreaming,
      tokensPerSecond: mockTokensPerSecond,
      duration: mockDuration,
      send: mockSendStream,
      cancel: mockCancelStream,
      reset: mockResetStream,
    } as any)

    vi.mocked(useChatSessionStore).mockReturnValue({
      currentSessionId: 'session-1',
      error: null,
      refreshSession: mockRefreshSession,
      updateSessionLocally: mockUpdateSessionLocally,
      updateSessionAgent: mockUpdateSessionAgent,
      updateSessionModel: mockUpdateSessionModel,
    } as any)

    vi.mocked(useModelsStore).mockReturnValue({
      loadModels: mockLoadModels,
      get getAllModels() {
        return mockModels
      },
    } as any)

    vi.mocked(listAgents).mockResolvedValue([
      {
        id: 'agent-1',
        name: 'Agent One',
      },
    ] as any)
  })

  it('syncs selected model when current session model changes with same id', async () => {
    const wrapper = mount(ChatPanel)
    await flushPromises()

    expect(wrapper.get('[data-testid="chatbox"]').attributes('data-selected-model')).toBe('gpt-4')

    mockCurrentSession.value = {
      ...mockCurrentSession.value!,
      model: 'gpt-5',
    }
    await nextTick()

    expect(wrapper.get('[data-testid="chatbox"]').attributes('data-selected-model')).toBe('gpt-5')
  })

  it('re-mounts chat box after async options load to refresh controlled select display', async () => {
    mount(ChatPanel)
    await flushPromises()

    expect(chatBoxMountCount).toBeGreaterThan(1)
  })
})
