import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import Workspace from '../Workspace.vue'

const mockListAgents = vi.fn()
const mockCreateSession = vi.fn()
const mockSelectSession = vi.fn()
const mockFetchSummaries = vi.fn()

let mockStore: any

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/api/agents', () => ({
  listAgents: (...args: unknown[]) => mockListAgents(...args),
  deleteAgent: vi.fn(),
}))

vi.mock('@/stores/chatSessionStore', () => ({
  useChatSessionStore: () => mockStore,
}))

vi.mock('@/composables/useTheme', () => ({
  useTheme: () => ({
    isDark: ref(false),
    toggleDark: vi.fn(),
  }),
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
  confirmDelete: vi.fn().mockResolvedValue(true),
}))

vi.mock('@/composables/workspace/useToolPanel', () => ({
  useToolPanel: () => ({
    visible: ref(false),
    activeEntry: ref(null),
    state: ref({ panelType: 'tool_result', title: '', toolName: null, data: null, step: null }),
    canNavigatePrev: ref(false),
    canNavigateNext: ref(false),
    handleShowPanelResult: vi.fn(),
    handleToolResult: vi.fn(),
    clearHistory: vi.fn(),
    closePanel: vi.fn(),
    navigateHistory: vi.fn(),
  }),
}))

vi.mock('@/components/workspace/SessionList.vue', () => ({
  default: defineComponent({
    name: 'SessionList',
    emits: ['newSession'],
    template: '<button data-testid="new-session" @click="$emit(\'newSession\')">new</button>',
  }),
}))

vi.mock('@/components/workspace/AgentList.vue', () => ({
  default: defineComponent({
    name: 'AgentList',
    emits: ['select'],
    template:
      '<div data-testid="agent-list"><button data-testid="select-agent" @click="$emit(\'select\', \'agent-1\')">select</button></div>',
  }),
}))

vi.mock('@/components/workspace/AgentEditorPanel.vue', () => ({
  default: defineComponent({
    name: 'AgentEditorPanel',
    emits: ['backToSessions'],
    template:
      '<div data-testid="agent-editor"><button data-testid="back-to-sessions" @click="$emit(\'backToSessions\')">back</button></div>',
  }),
}))

vi.mock('@/components/chat/ChatPanel.vue', () => ({
  default: defineComponent({
    name: 'ChatPanel',
    template: '<div data-testid="chat-panel" />',
  }),
}))

vi.mock('@/components/tool-panel/ToolPanel.vue', () => ({
  default: defineComponent({
    name: 'ToolPanel',
    template: '<div data-testid="tool-panel" />',
  }),
}))

vi.mock('@/components/settings/SettingsPanel.vue', () => ({
  default: defineComponent({
    name: 'SettingsPanel',
    template: '<div data-testid="settings-panel" />',
  }),
}))

vi.mock('@/components/workspace/ConvertToBackgroundAgentDialog.vue', () => ({
  default: defineComponent({
    name: 'ConvertToBackgroundAgentDialog',
    template: '<div data-testid="convert-dialog" />',
  }),
}))

vi.mock('@/components/workspace/CreateAgentDialog.vue', () => ({
  default: defineComponent({
    name: 'CreateAgentDialog',
    template: '<div data-testid="create-agent-dialog" />',
  }),
}))

function createSession(id: string) {
  return {
    id,
    name: 'Session',
    agent_id: 'agent-1',
    model: 'gpt-5',
    skill_id: null,
    messages: [],
    created_at: 1n,
    updated_at: 1n,
    summary_message_id: null,
    prompt_tokens: 0n,
    completion_tokens: 0n,
    cost: 0,
    metadata: { total_tokens: 0, message_count: 0, last_model: null },
    source_channel: null,
    source_conversation_id: null,
  }
}

describe('Workspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()

    mockStore = {
      summaries: [],
      currentSession: null,
      currentSessionId: null,
      isSending: false,
      error: null,
      createSession: mockCreateSession,
      selectSession: mockSelectSession,
      deleteSession: vi.fn().mockResolvedValue(true),
      renameSession: vi.fn().mockResolvedValue(null),
      fetchSummaries: mockFetchSummaries,
    }

    mockListAgents.mockResolvedValue([
      {
        id: 'agent-1',
        name: 'Agent One',
        agent: { model: 'gpt-5' },
      },
    ])

    mockCreateSession.mockResolvedValue(createSession('session-new'))
    mockSelectSession.mockResolvedValue(undefined)
    mockFetchSummaries.mockResolvedValue(undefined)
  })

  it('creates and selects a new session immediately when clicking new session', async () => {
    const wrapper = mount(Workspace, {
      global: {
        stubs: {
          Button: {
            template: '<button><slot /></button>',
          },
          Dialog: {
            template: '<div><slot /></div>',
          },
          DialogContent: {
            template: '<div><slot /></div>',
          },
          DialogHeader: {
            template: '<div><slot /></div>',
          },
          DialogTitle: {
            template: '<div><slot /></div>',
          },
          DialogFooter: {
            template: '<div><slot /></div>',
          },
          Input: {
            template: '<input />',
          },
        },
      },
    })

    await flushPromises()
    await wrapper.get('[data-testid="new-session"]').trigger('click')

    expect(mockCreateSession).toHaveBeenCalledWith('agent-1', 'gpt-5')
    expect(mockSelectSession).toHaveBeenCalledWith('session-new')
  })

  it('switches to agent tab, opens editor, and can switch back to sessions', async () => {
    const wrapper = mount(Workspace, {
      global: {
        stubs: {
          Button: {
            template: '<button><slot /></button>',
          },
          Dialog: {
            template: '<div><slot /></div>',
          },
          DialogContent: {
            template: '<div><slot /></div>',
          },
          DialogHeader: {
            template: '<div><slot /></div>',
          },
          DialogTitle: {
            template: '<div><slot /></div>',
          },
          DialogFooter: {
            template: '<div><slot /></div>',
          },
          Input: {
            template: '<input />',
          },
        },
      },
    })

    await flushPromises()

    const tabAgents = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.tabs.agents'))
    expect(tabAgents).toBeDefined()
    await tabAgents!.trigger('click')

    expect(wrapper.find('[data-testid="agent-list"]').exists()).toBe(true)

    await wrapper.get('[data-testid="select-agent"]').trigger('click')
    expect(wrapper.find('[data-testid="agent-editor"]').exists()).toBe(true)

    await wrapper.get('[data-testid="back-to-sessions"]').trigger('click')
    expect(wrapper.find('[data-testid="chat-panel"]').exists()).toBe(true)
  })
})
