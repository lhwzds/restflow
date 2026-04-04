import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h, nextTick, ref } from 'vue'
import ChatPanel from '../ChatPanel.vue'
import { useChatSession } from '@/composables/workspace/useChatSession'
import { useChatStream } from '@/composables/workspace/useChatStream'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useTaskStore } from '@/stores/taskStore'
import { useModelsStore } from '@/stores/modelsStore'
import { listAgents, getAgent, updateAgent } from '@/api/agents'
import { steerChatStream } from '@/api/chat-stream'
import { sendChatMessage } from '@/api/chat-session'
import { getExecutionRunThread, listExecutionContainers, listRuns } from '@/api/execution-console'

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

type MockStreamState = {
  messageId: string | null
  content: string
  thinking: string
  steps: any[]
  isStreaming: boolean
  error: string | null
  tokenCount: number
  inputTokens: number
  outputTokens: number
  startedAt: number | null
  completedAt: number | null
  acknowledgement: string
}

const mockCurrentSession = ref<SessionLike | null>(null)
const mockMessages = ref<any[]>([])
const mockIsSending = ref(false)

const mockStreamState = ref<MockStreamState>({
  messageId: null as string | null,
  content: '',
  thinking: '',
  steps: [] as any[],
  isStreaming: false,
  error: null as string | null,
  tokenCount: 0,
  inputTokens: 0,
  outputTokens: 0,
  startedAt: null,
  completedAt: null,
  acknowledgement: '',
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
const mockConfirm = vi.fn()
const mockGetAgentApi = vi.fn()
const mockUpdateAgentApi = vi.fn()
const mockModels: Array<{ model: string; name: string; provider: string }> = []

let chatBoxMountCount = 0
const mockSteerChatStream = vi.fn()
const mockSendChatMessageApi = vi.fn()
const mockRouterPush = vi.fn()
const mockRouterReplace = vi.fn()
const mockGetExecutionRunThread = vi.fn()
const mockListExecutionContainers = vi.fn()
const mockListRuns = vi.fn()
let lastMessageListProps: Record<string, unknown> | null = null
let lastExecutionStatusBarProps: Record<string, unknown> | null = null

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('vue-router', () => ({
  useRouter: () => ({
    push: (...args: unknown[]) => mockRouterPush(...args),
    replace: (...args: unknown[]) => mockRouterReplace(...args),
  }),
}))

vi.mock('@/components/chat/MessageList.vue', () => ({
  default: defineComponent({
    name: 'MessageList',
    props: {
      messages: {
        type: Array,
        default: () => [],
      },
      threadItems: {
        type: Array,
        default: undefined,
      },
      isStreaming: {
        type: Boolean,
        default: false,
      },
      streamContent: {
        type: String,
        default: '',
      },
      streamThinking: {
        type: String,
        default: '',
      },
      steps: {
        type: Array,
        default: () => [],
      },
    },
    setup(props) {
      return () => {
        lastMessageListProps = {
          messages: props.messages,
          threadItems: props.threadItems,
          isStreaming: props.isStreaming,
          streamContent: props.streamContent,
          streamThinking: props.streamThinking,
          steps: props.steps,
        }
        return h('div', { 'data-testid': 'message-list' })
      }
    },
  }),
}))

vi.mock('@/components/chat/ExecutionStatusBar.vue', () => ({
  default: defineComponent({
    name: 'ExecutionStatusBar',
    props: {
      isActive: {
        type: Boolean,
        default: false,
      },
      startedAt: {
        type: Number,
        default: null,
      },
      steps: {
        type: Array,
        default: () => [],
      },
      fallbackLabel: {
        type: String,
        default: null,
      },
    },
    setup(props) {
      return () => {
        lastExecutionStatusBarProps = {
          isActive: props.isActive,
          startedAt: props.startedAt,
          steps: props.steps,
          fallbackLabel: props.fallbackLabel,
        }
        return h('div', { 'data-testid': 'execution-status-bar' })
      }
    },
  }),
}))

vi.mock('@/components/task/TaskStatusBadge.vue', () => ({
  default: {
    name: 'TaskStatusBadge',
    template: '<span data-testid="task-status-badge" />',
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
    emits: ['send', 'update:selectedModel', 'send-voice-message'],
    setup(props, { emit }) {
      chatBoxMountCount += 1
      return () =>
        h(
          'div',
          {
            'data-testid': 'chatbox',
            'data-selected-model': props.selectedModel ?? '',
          },
          [
            props.selectedModel ?? '',
            h(
              'button',
              {
                'data-testid': 'chatbox-send',
                onClick: () => emit('send', 'follow-up'),
              },
              'send',
            ),
            h(
              'button',
              {
                'data-testid': 'chatbox-model-change',
                onClick: () => emit('update:selectedModel', 'gpt-5'),
              },
              'model',
            ),
            h(
              'button',
              {
                'data-testid': 'chatbox-send-voice',
                onClick: () =>
                  emit('send-voice-message', {
                    filePath: '/tmp/voice-message.webm',
                    audioBlobUrl: 'blob:test',
                    durationSec: 4,
                  }),
              },
              'voice',
            ),
          ],
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

vi.mock('@/stores/taskStore', () => ({
  useTaskStore: vi.fn(),
}))

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: vi.fn(),
}))

vi.mock('@/api/agents', () => ({
  listAgents: vi.fn(),
  getAgent: vi.fn(),
  updateAgent: vi.fn(),
}))

vi.mock('@/api/chat-session', () => ({
  sendChatMessage: vi.fn(),
}))

vi.mock('@/api/chat-stream', () => ({
  steerChatStream: vi.fn(),
}))

vi.mock('@/api/execution-console', () => ({
  getExecutionRunThread: vi.fn(),
  listExecutionContainers: vi.fn(),
  listRuns: vi.fn(),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: (...args: unknown[]) => mockConfirm(...args),
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
    lastMessageListProps = null
    lastExecutionStatusBarProps = null

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
      startedAt: null,
      completedAt: null,
      acknowledgement: '',
    }
    mockIsStreaming.value = false
    mockTokensPerSecond.value = 0
    mockDuration.value = 0
    mockSendStream.mockResolvedValue('run-live-1')

    mockModels.length = 0
    mockConfirm.mockResolvedValue(true)
    mockLoadModels.mockImplementation(async () => {
      mockModels.splice(
        0,
        mockModels.length,
        { model: 'gpt-4', name: 'GPT-4', provider: 'openai' },
        { model: 'gpt-5', name: 'GPT-5', provider: 'openai' },
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
      currentSession: mockCurrentSession.value,
    } as any)

    vi.mocked(useTaskStore).mockReturnValue({
      agents: [],
      tasks: [],
      fetchTasks: vi.fn(),
      fetchAgents: vi.fn(),
      taskBySessionId: () => null,
      agentBySessionId: () => null,
      pauseTask: vi.fn(),
      resumeTask: vi.fn(),
      runTaskNow: vi.fn(),
      stopTask: vi.fn(),
    } as any)

    vi.mocked(useModelsStore).mockReturnValue({
      loadModels: mockLoadModels,
      get getAllModels() {
        return mockModels
      },
      getModelMetadata: (model: string) => mockModels.find((item) => item.model === model),
    } as any)

    vi.mocked(listAgents).mockResolvedValue([
      {
        id: 'agent-1',
        name: 'Agent One',
      },
    ] as any)
    mockSteerChatStream.mockResolvedValue(true)
    mockRouterPush.mockReset()
    mockRouterReplace.mockReset()
    mockSendChatMessageApi.mockResolvedValue(mockCurrentSession.value)
    mockGetAgentApi.mockResolvedValue({
      id: 'agent-1',
      name: 'Agent One',
      agent: {
        model: 'gpt-4',
        model_ref: {
          provider: 'openai',
          model: 'gpt-4',
        },
      },
    })
    mockUpdateAgentApi.mockResolvedValue({
      id: 'agent-1',
      name: 'Agent One',
      agent: {
        model: 'gpt-5',
        model_ref: {
          provider: 'openai',
          model: 'gpt-5',
        },
      },
    })
    vi.mocked(getAgent).mockImplementation(mockGetAgentApi)
    vi.mocked(updateAgent).mockImplementation(mockUpdateAgentApi)
    vi.mocked(steerChatStream).mockImplementation(mockSteerChatStream)
    vi.mocked(sendChatMessage).mockImplementation(mockSendChatMessageApi)
    mockGetExecutionRunThread.mockResolvedValue({
      focus: {
        run_id: 'run-1',
        container_id: 'session-1',
        session_id: 'session-1',
        started_at: 1000,
        updated_at: 3000,
        ended_at: 3000,
        kind: 'workspace_run',
      },
      timeline: {
        events: [],
        stats: {},
      },
    })
    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'session-1',
        kind: 'workspace',
        title: 'Workspace Session',
        subtitle: null,
        status: 'completed',
        updated_at: 1234,
        session_count: 1,
        latest_session_id: 'session-1',
        latest_run_id: 'run-1',
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
      },
    ])
    mockListRuns.mockResolvedValue([])
    vi.mocked(getExecutionRunThread).mockImplementation(mockGetExecutionRunThread)
    vi.mocked(listExecutionContainers).mockImplementation(mockListExecutionContainers)
    vi.mocked(listRuns).mockImplementation(mockListRuns)
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

  it('disables recent auto-selection when the panel is route-driven', async () => {
    mount(ChatPanel, {
      props: {
        selectedRunId: 'run-1',
        autoSelectRecent: false,
      },
    })
    await flushPromises()

    expect(useChatSession).toHaveBeenCalledWith({
      autoLoad: true,
      autoSelectRecent: false,
    })
  })

  it('re-mounts chat box after async options load to refresh controlled select display', async () => {
    mount(ChatPanel)
    await flushPromises()

    expect(chatBoxMountCount).toBeGreaterThan(1)
  })

  it('updates model selector when model options arrive after initial mount', async () => {
    mockModels.length = 0
    mockLoadModels.mockImplementation(async () => {
      // Simulate loadModels returning while another caller is already loading.
    })

    const wrapper = mount(ChatPanel)
    await flushPromises()

    expect(wrapper.get('[data-testid="chatbox"]').attributes('data-selected-model')).toBe('gpt-4')

    mockModels.push(
      { model: 'gpt-4', name: 'GPT-4', provider: 'openai' },
      { model: 'gpt-5', name: 'GPT-5', provider: 'openai' },
    )
    await nextTick()

    expect(wrapper.get('[data-testid="chatbox"]').attributes('data-selected-model')).toBe('gpt-4')
    expect(chatBoxMountCount).toBeGreaterThan(1)
  })

  it('steers active stream and persists user follow-up without starting a new stream', async () => {
    mockIsStreaming.value = true
    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-send"]').trigger('click')
    await flushPromises()

    expect(mockSteerChatStream).toHaveBeenCalledWith('session-1', 'follow-up')
    expect(mockSendChatMessageApi).toHaveBeenCalledWith('session-1', 'follow-up')
    expect(mockSendStream).not.toHaveBeenCalled()
  })

  it('falls back to a new stream when no active steerable stream exists', async () => {
    mockIsStreaming.value = true
    mockSteerChatStream.mockResolvedValue(false)

    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-send"]').trigger('click')
    await flushPromises()

    expect(mockSteerChatStream).toHaveBeenCalledWith('session-1', 'follow-up')
    expect(mockSendChatMessageApi).not.toHaveBeenCalled()
    expect(mockSendStream).toHaveBeenCalledWith('follow-up')
  })

  it('emits a canonical runStarted event as soon as streaming begins', async () => {
    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-send"]').trigger('click')
    await flushPromises()

    expect(mockSendStream).toHaveBeenCalledWith('follow-up')
    expect(wrapper.emitted('runStarted')).toEqual([
      [{ containerId: 'session-1', runId: 'run-live-1' }],
    ])
  })

  it('emits the active container id when streaming starts inside a non-workspace container', async () => {
    const wrapper = mount(ChatPanel, {
      props: {
        containerId: 'task-1',
      },
    })
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-send"]').trigger('click')
    await flushPromises()

    expect(wrapper.emitted('runStarted')).toEqual([
      [{ containerId: 'task-1', runId: 'run-live-1' }],
    ])
  })

  it('shows the execution status bar during the pre-stream sending phase', async () => {
    mockIsSending.value = true

    const wrapper = mount(ChatPanel)
    await flushPromises()

    expect(wrapper.find('[data-testid="execution-status-bar"]').exists()).toBe(true)
    expect(lastExecutionStatusBarProps?.isActive).toBe(true)
    expect(lastExecutionStatusBarProps?.fallbackLabel).toBe('Preparing run...')
    expect(typeof lastExecutionStatusBarProps?.startedAt).toBe('number')
  })

  it('sends normalized voice content without transcribe instruction', async () => {
    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-send-voice"]').trigger('click')
    await flushPromises()

    expect(mockSendStream).toHaveBeenCalledWith(
      '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice-message.webm',
    )
  })

  it('persists updated agent model with model_ref when session model changes', async () => {
    mockUpdateSessionModel.mockResolvedValue({
      ...mockCurrentSession.value!,
      model: 'gpt-5',
    })

    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-model-change"]').trigger('click')
    await flushPromises()

    expect(mockUpdateSessionModel).toHaveBeenCalledWith('session-1', 'gpt-5')
    expect(mockGetAgentApi).toHaveBeenCalledWith('agent-1')
    expect(mockUpdateAgentApi).toHaveBeenCalledWith('agent-1', {
      agent: {
        model: 'gpt-5',
        model_ref: {
          provider: 'openai',
          model: 'gpt-5',
        },
      },
    })
  })

  it('persists agent model directly without confirmation retry', async () => {
    mockUpdateSessionModel.mockResolvedValue({
      ...mockCurrentSession.value!,
      model: 'gpt-5',
    })
    mockUpdateAgentApi.mockResolvedValueOnce({
        id: 'agent-1',
        name: 'Agent One',
        agent: {
          model: 'gpt-5',
          model_ref: {
            provider: 'openai',
            model: 'gpt-5',
          },
        },
      })

    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="chatbox-model-change"]').trigger('click')
    await flushPromises()

    expect(mockConfirm).not.toHaveBeenCalled()
    expect(mockUpdateAgentApi).toHaveBeenCalledTimes(1)
  })

  it('emits toolResult for failed tool calls with result payload', async () => {
    const wrapper = mount(ChatPanel)
    await flushPromises()

    mockStreamState.value.steps = [
      {
        type: 'tool_call',
        name: 'browser',
        status: 'failed',
        toolId: 'tool-1',
        result: 'Error: Chromium executable not found',
      },
    ]
    await nextTick()

    const emitted = wrapper.emitted('toolResult')
    expect(emitted).toBeTruthy()
    expect(emitted![0]?.[0]).toMatchObject({
      name: 'browser',
      status: 'failed',
      toolId: 'tool-1',
    })
  })

  it('shows a run trace entry for linked background sessions', async () => {
    vi.mocked(useTaskStore).mockReturnValue({
      agents: [],
      tasks: [],
      fetchTasks: vi.fn(),
      fetchAgents: vi.fn(),
      taskBySessionId: () => ({
        id: 'task-1',
        status: 'running',
        chat_session_id: 'session-1',
      }),
      agentBySessionId: () => ({
        id: 'task-1',
        status: 'running',
        chat_session_id: 'session-1',
      }),
      pauseTask: vi.fn(),
      resumeTask: vi.fn(),
      runTaskNow: vi.fn(),
      stopTask: vi.fn(),
    } as any)
    mockListRuns.mockResolvedValue([
      {
        id: 'run-summary-1',
        container_id: 'task-1',
        title: 'Run 1',
        status: 'completed',
        updated_at: 1234,
        run_id: 'run-1',
      },
    ])

    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="open-run-trace"]').trigger('click')
    await flushPromises()

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'task-1', runId: 'run-1' },
    })
  })

  it('falls back to task route when linked background session has no runs yet', async () => {
    vi.mocked(useTaskStore).mockReturnValue({
      agents: [],
      tasks: [],
      fetchTasks: vi.fn(),
      fetchAgents: vi.fn(),
      taskBySessionId: () => ({
        id: 'task-1',
        status: 'running',
        chat_session_id: 'session-1',
      }),
      agentBySessionId: () => ({
        id: 'task-1',
        status: 'running',
        chat_session_id: 'session-1',
      }),
      pauseTask: vi.fn(),
      resumeTask: vi.fn(),
      runTaskNow: vi.fn(),
      stopTask: vi.fn(),
    } as any)
    mockListRuns.mockResolvedValue([])

    const wrapper = mount(ChatPanel)
    await flushPromises()

    await wrapper.get('[data-testid="open-run-trace"]').trigger('click')
    await flushPromises()

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container',
      params: { containerId: 'task-1' },
    })
  })

  it('loads canonical execution thread for the current session and passes unified items to MessageList', async () => {
    mockMessages.value = [
      {
        id: 'msg-user-1',
        role: 'user',
        content: 'Find the latest release notes',
        timestamp: 1000n,
        execution: null,
      },
      {
        id: 'msg-assistant-1',
        role: 'assistant',
        content: 'I found the release notes and summarized the changes in detail.',
        timestamp: 3000n,
        execution: null,
      },
    ]
    mockGetExecutionRunThread.mockResolvedValue({
      focus: {
        run_id: 'run-1',
        container_id: 'session-1',
        session_id: 'session-1',
        started_at: 1000,
        updated_at: 3000,
        ended_at: 3000,
        kind: 'workspace_run',
      },
      timeline: {
        events: [
          {
            id: 'event-user-1',
            task_id: 'task-1',
            agent_id: 'agent-1',
            category: 'message',
            source: 'agent_executor',
            timestamp: 1000,
            subflow_path: [],
            run_id: null,
            parent_run_id: null,
            session_id: 'session-1',
            turn_id: 'turn-1',
            requested_model: 'gpt-5',
            effective_model: 'gpt-5',
            provider: 'openai',
            attempt: 1,
            llm_call: null,
            tool_call: null,
            model_switch: null,
            lifecycle: null,
            message: {
              role: 'user',
              content_preview: 'Find the latest release notes',
              tool_call_count: null,
            },
            metric_sample: null,
            provider_health: null,
            log_record: null,
          },
          {
            id: 'event-tool-1',
            task_id: 'task-1',
            agent_id: 'agent-1',
            category: 'tool_call',
            source: 'agent_executor',
            timestamp: 2000,
            subflow_path: [],
            run_id: null,
            parent_run_id: null,
            session_id: 'session-1',
            turn_id: 'turn-1',
            requested_model: 'gpt-5',
            effective_model: 'gpt-5',
            provider: 'openai',
            attempt: 1,
            llm_call: null,
            tool_call: {
              tool_name: 'web_search',
              phase: 'completed',
              input_summary: 'release notes',
              output_ref: null,
              error: null,
            },
            model_switch: null,
            lifecycle: null,
            message: null,
            metric_sample: null,
            provider_health: null,
            log_record: null,
          },
          {
            id: 'event-assistant-1',
            task_id: 'task-1',
            agent_id: 'agent-1',
            category: 'message',
            source: 'agent_executor',
            timestamp: 3000,
            subflow_path: [],
            run_id: null,
            parent_run_id: null,
            session_id: 'session-1',
            turn_id: 'turn-1',
            requested_model: 'gpt-5',
            effective_model: 'gpt-5',
            provider: 'openai',
            attempt: 1,
            llm_call: null,
            tool_call: null,
            model_switch: null,
            lifecycle: null,
            message: {
              role: 'assistant',
              content_preview: 'I found the release notes',
              tool_call_count: 1,
            },
            metric_sample: null,
            provider_health: null,
            log_record: null,
          },
        ],
        stats: {},
      },
    } as any)

    mount(ChatPanel, {
      props: {
        selectedRunId: 'run-1',
      },
    })
    await flushPromises()

    expect(getExecutionRunThread).toHaveBeenCalledWith('run-1')
    expect(lastMessageListProps?.threadItems).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'run-group-turn-1',
          kind: 'run_group',
          children: expect.arrayContaining([
            expect.objectContaining({
              id: 'event-tool-1',
              kind: 'tool_call',
              title: 'web_search',
            }),
          ]),
        }),
        expect.objectContaining({
          kind: 'message',
          message: expect.objectContaining({
            id: 'msg-assistant-1',
            content: 'I found the release notes and summarized the changes in detail.',
          }),
        }),
      ]),
    )
  })

  it('keeps selected run mode scoped to the active run instead of appending unrelated transcript messages', async () => {
    mockMessages.value = [
      {
        id: 'msg-old',
        role: 'assistant',
        content: 'Older transcript from another run',
        timestamp: 500n,
        execution: null,
      },
      {
        id: 'msg-run-1',
        role: 'assistant',
        content: 'Current run assistant reply',
        timestamp: 2500n,
        execution: null,
      },
    ]
    mockGetExecutionRunThread.mockResolvedValue({
      focus: {
        run_id: 'run-1',
        container_id: 'session-1',
        session_id: 'session-1',
        started_at: 1000,
        updated_at: 3000,
        ended_at: 3000,
        kind: 'workspace_run',
      },
      timeline: {
        events: [
          {
            id: 'event-tool-1',
            task_id: 'task-1',
            agent_id: 'agent-1',
            category: 'tool_call',
            source: 'agent_executor',
            timestamp: 2000,
            subflow_path: [],
            run_id: 'run-1',
            parent_run_id: null,
            session_id: 'session-1',
            turn_id: 'turn-1',
            requested_model: 'gpt-5',
            effective_model: 'gpt-5',
            provider: 'openai',
            attempt: 1,
            llm_call: null,
            tool_call: {
              tool_call_id: 'tool-call-1',
              tool_name: 'bash',
              phase: 'completed',
              input: null,
              input_summary: 'echo hello',
              output: 'hello',
              output_ref: null,
              success: true,
              error: null,
              duration_ms: 100n,
            },
            model_switch: null,
            lifecycle: null,
            message: null,
            metric_sample: null,
            provider_health: null,
            log_record: null,
          },
        ],
        stats: {},
      },
    } as any)

    mount(ChatPanel, {
      props: {
        selectedRunId: 'run-1',
      },
    })
    await flushPromises()

    const threadItems = (lastMessageListProps?.threadItems ?? []) as Array<{ id: string; message?: { id: string } }>
    expect(threadItems.some((item) => item.id === 'msg-old')).toBe(false)
    expect(threadItems.some((item) => item.id === 'msg-run-1')).toBe(true)
  })

  it('uses adjacent container runs to keep neighboring run messages out of the current run view', async () => {
    mockMessages.value = [
      {
        id: 'msg-prev-run',
        role: 'assistant',
        content: 'Previous run output',
        timestamp: 800n,
        execution: null,
      },
      {
        id: 'msg-run-1',
        role: 'assistant',
        content: 'Current run assistant reply',
        timestamp: 2500n,
        execution: null,
      },
      {
        id: 'msg-next-run',
        role: 'assistant',
        content: 'Next run output',
        timestamp: 4500n,
        execution: null,
      },
    ]
    mockListRuns.mockResolvedValue([
      {
        id: 'run-summary-0',
        kind: 'workspace_run',
        container_id: 'session-1',
        title: 'Previous run',
        status: 'completed',
        updated_at: 900,
        started_at: 100,
        ended_at: 900,
        run_id: 'run-0',
        event_count: 2,
      },
      {
        id: 'run-summary-1',
        kind: 'workspace_run',
        container_id: 'session-1',
        title: 'Current run',
        status: 'completed',
        updated_at: 3000,
        started_at: 1000,
        ended_at: 3000,
        run_id: 'run-1',
        event_count: 3,
      },
      {
        id: 'run-summary-2',
        kind: 'workspace_run',
        container_id: 'session-1',
        title: 'Next run',
        status: 'completed',
        updated_at: 5200,
        started_at: 4000,
        ended_at: 5200,
        run_id: 'run-2',
        event_count: 2,
      },
    ] as any)
    mockGetExecutionRunThread.mockResolvedValue({
      focus: {
        run_id: 'run-1',
        container_id: 'session-1',
        session_id: 'session-1',
        started_at: 1000,
        updated_at: 3000,
        ended_at: 3000,
        kind: 'workspace_run',
      },
      timeline: {
        events: [
          {
            id: 'event-tool-1',
            task_id: 'task-1',
            agent_id: 'agent-1',
            category: 'tool_call',
            source: 'agent_executor',
            timestamp: 2000,
            subflow_path: [],
            run_id: 'run-1',
            parent_run_id: null,
            session_id: 'session-1',
            turn_id: 'turn-1',
            requested_model: 'gpt-5',
            effective_model: 'gpt-5',
            provider: 'openai',
            attempt: 1,
            llm_call: null,
            tool_call: {
              tool_call_id: 'tool-call-1',
              tool_name: 'bash',
              phase: 'completed',
              input: null,
              input_summary: 'echo hello',
              output: 'hello',
              output_ref: null,
              success: true,
              error: null,
              duration_ms: 100n,
            },
            model_switch: null,
            lifecycle: null,
            message: null,
            metric_sample: null,
            provider_health: null,
            log_record: null,
          },
        ],
        stats: {},
      },
    } as any)

    mount(ChatPanel, {
      props: {
        selectedRunId: 'run-1',
      },
    })
    await flushPromises()

    expect(mockListRuns).toHaveBeenCalledWith({
      container: {
        kind: 'workspace',
        id: 'session-1',
      },
    })

    const threadItems = (lastMessageListProps?.threadItems ?? []) as Array<{ id: string; message?: { id: string } }>
    expect(threadItems.some((item) => item.id === 'msg-prev-run')).toBe(false)
    expect(threadItems.some((item) => item.id === 'msg-run-1')).toBe(true)
    expect(threadItems.some((item) => item.id === 'msg-next-run')).toBe(false)
  })

  it('shows a full breadcrumb chain for child runs and navigates to root and parent runs', async () => {
    mockGetExecutionRunThread.mockImplementation(async (runId: string) => {
      if (runId === 'run-root') {
        return {
          focus: {
            id: 'run-root',
            kind: 'workspace_run',
            container_id: 'session-1',
            root_run_id: 'run-root',
            title: 'Root run',
            subtitle: null,
            status: 'completed',
            updated_at: 1000,
            started_at: 900,
            ended_at: 1000,
            session_id: 'session-1',
            run_id: 'run-root',
            task_id: null,
            parent_run_id: null,
            agent_id: 'agent-1',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: 'openai',
            event_count: 1,
          },
          timeline: { events: [], stats: {} },
        } as any
      }

      if (runId === 'run-parent') {
        return {
          focus: {
            id: 'run-parent',
            kind: 'subagent_run',
            container_id: 'session-1',
            root_run_id: 'run-root',
            title: 'Planner run',
            subtitle: null,
            status: 'completed',
            updated_at: 1500,
            started_at: 1000,
            ended_at: 1500,
            session_id: 'session-1',
            run_id: 'run-parent',
            task_id: null,
            parent_run_id: 'run-root',
            agent_id: 'agent-1',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: 'openai',
            event_count: 1,
          },
          timeline: { events: [], stats: {} },
        } as any
      }

      return {
        focus: {
          id: 'run-child',
          kind: 'subagent_run',
          container_id: 'session-1',
          root_run_id: 'run-root',
          title: 'Child run',
          subtitle: null,
          status: 'completed',
          updated_at: 2000,
          started_at: 1000,
          ended_at: 2000,
          session_id: 'session-1',
          run_id: 'run-child',
          task_id: null,
          parent_run_id: 'run-parent',
          agent_id: 'agent-1',
          source_channel: 'workspace',
          source_conversation_id: null,
          effective_model: 'gpt-5',
          provider: 'openai',
          event_count: 1,
        },
        timeline: {
          events: [],
          stats: {},
        },
      } as any
    })

    const wrapper = mount(ChatPanel, {
      props: {
        selectedRunId: 'run-child',
      },
    })
    await flushPromises()

    expect(wrapper.get('[data-testid="run-breadcrumb"]').text()).toContain('Root')
    expect(wrapper.get('[data-testid="run-breadcrumb"]').text()).toContain('Parent')
    expect(wrapper.get('[data-testid="run-breadcrumb"]').text()).toContain('Child')
    expect(wrapper.get('[data-testid="run-breadcrumb-node-root"]').text()).toContain('Root run')
    expect(wrapper.get('[data-testid="run-breadcrumb-node-parent"]').text()).toContain('Planner run')
    expect(wrapper.get('[data-testid="run-breadcrumb"]').text()).toContain('Agent One')
    expect(wrapper.get('[data-testid="run-breadcrumb-current"]').text()).toContain('Child run')

    await wrapper.get('[data-testid="run-breadcrumb-node-root"]').trigger('click')

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-root' },
    })

    await wrapper.get('[data-testid="run-breadcrumb-node-parent"]').trigger('click')

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-parent' },
    })
  })

  it('does not request canonical thread data before a run exists', async () => {
    mount(ChatPanel)
    await flushPromises()

    expect(getExecutionRunThread).not.toHaveBeenCalled()
  })

  it('normalizes a fresh workspace session into the canonical container run route after streaming completes', async () => {
    mockRefreshSession.mockResolvedValue(mockCurrentSession.value)

    mount(ChatPanel)
    await flushPromises()

    mockIsStreaming.value = true
    await nextTick()
    mockIsStreaming.value = false
    await flushPromises()

    expect(mockRefreshSession).toHaveBeenCalledWith('session-1')
    expect(mockListExecutionContainers).toHaveBeenCalled()
    expect(mockRouterReplace).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-1' },
    })
  })

  it('normalizes a background-linked session into the canonical background container run route after streaming completes', async () => {
    mockCurrentSession.value = {
      ...mockCurrentSession.value!,
      source_channel: 'workspace',
    } as any
    mockRefreshSession.mockResolvedValue(mockCurrentSession.value)
    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'task-1',
        kind: 'background_task',
        title: 'Digest Agent',
        subtitle: null,
        status: 'completed',
        updated_at: 1234,
        session_count: 1,
        latest_session_id: 'session-1',
        latest_run_id: null,
        agent_id: 'agent-1',
        source_channel: null,
        source_conversation_id: null,
      },
    ])
    mockListRuns.mockResolvedValue([
      {
        id: 'run-summary-1',
        container_id: 'task-1',
        title: 'Run 1',
        status: 'completed',
        updated_at: 1234,
        run_id: 'run-1',
      },
    ])

    mount(ChatPanel)
    await flushPromises()

    mockIsStreaming.value = true
    await nextTick()
    mockIsStreaming.value = false
    await flushPromises()

    expect(mockListRuns).toHaveBeenCalledWith({
      container: {
        kind: 'background_task',
        id: 'task-1',
      },
    })
    expect(mockRouterReplace).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'task-1', runId: 'run-1' },
    })
  })
})
