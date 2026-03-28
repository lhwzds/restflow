import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, reactive, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import Workspace from '../Workspace.vue'
import { BackendError } from '@/api/http-client'

const mockListAgents = vi.fn()
const mockRouterPush = vi.fn()
const mockRouterReplace = vi.fn()
const mockCreateSession = vi.fn()
const mockSelectSession = vi.fn()
const mockFetchSummaries = vi.fn()
const mockFetchBackgroundAgents = vi.fn()
const mockListExecutionContainers = vi.fn()
const mockListExecutionSessions = vi.fn()
const mockGetExecutionRunThread = vi.fn()
const mockRoute = reactive<{ name: string; params: Record<string, string>; query: Record<string, string> }>({
  name: 'workspace',
  params: {},
  query: {},
})

let mockStore: any
let mockBackgroundStore: any

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
  useRoute: () => mockRoute,
}))

vi.mock('@/api/agents', () => ({
  listAgents: (...args: unknown[]) => mockListAgents(...args),
  deleteAgent: vi.fn(),
}))

vi.mock('@/api/execution-console', () => ({
  listExecutionContainers: (...args: unknown[]) => mockListExecutionContainers(...args),
  listExecutionSessions: (...args: unknown[]) => mockListExecutionSessions(...args),
  getExecutionRunThread: (...args: unknown[]) => mockGetExecutionRunThread(...args),
}))

vi.mock('@/stores/chatSessionStore', () => ({
  useChatSessionStore: () => mockStore,
}))

vi.mock('@/stores/backgroundAgentStore', () => ({
  useBackgroundAgentStore: () => mockBackgroundStore,
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
  useConfirm: () => ({
    confirm: vi.fn().mockResolvedValue(true),
  }),
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
    handleThreadSelection: vi.fn(),
    clearHistory: vi.fn(),
    closePanel: vi.fn(),
    navigateHistory: vi.fn(),
  }),
}))

vi.mock('@/components/workspace/SessionList.vue', () => ({
  default: defineComponent({
    name: 'SessionList',
    emits: ['newSession', 'selectRun', 'selectContainer'],
    template:
      '<div><button data-testid="new-session" @click="$emit(\'newSession\')">new</button><button data-testid="select-run" @click="$emit(\'selectRun\', \'session-1\', \'run-1\')">run</button><button data-testid="select-container" @click="$emit(\'selectContainer\', \'workspace\', \'session-1\')">container</button></div>',
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
    emits: ['threadLoaded', 'threadSelection'],
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

function mountWorkspace() {
  return mount(Workspace, {
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
}

describe('Workspace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockRouterPush.mockReset()
    mockRouterReplace.mockReset()
    window.localStorage.clear()
    mockRoute.name = 'workspace'
    mockRoute.params = {}
    mockRoute.query = {}

    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'session-1',
        kind: 'workspace',
        title: 'Workspace Session',
        subtitle: 'Latest reply',
        updated_at: 1,
        status: 'completed',
        session_count: 0,
        latest_session_id: 'session-1',
        latest_run_id: 'run-1',
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
      },
    ])
    mockListExecutionSessions.mockResolvedValue([])
    mockGetExecutionRunThread.mockResolvedValue({
      focus: {
        id: 'run-1',
        kind: 'workspace_run',
        container_id: 'session-1',
        title: 'Run #1',
        subtitle: null,
        status: 'completed',
        updated_at: 1,
        started_at: 1,
        ended_at: 2,
        session_id: 'session-1',
        run_id: 'run-1',
        task_id: null,
        parent_run_id: null,
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
        effective_model: 'gpt-5',
        provider: null,
        event_count: 2,
      },
      timeline: { events: [], stats: {} },
      child_sessions: [],
    })

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
    mockBackgroundStore = {
      agents: [],
      fetchAgents: mockFetchBackgroundAgents,
      agentBySessionId: vi.fn(() => null),
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
    mockFetchBackgroundAgents.mockResolvedValue(undefined)
  })

  it('creates and selects a new session immediately when clicking new session', async () => {
    const wrapper = mountWorkspace()

    await flushPromises()
    await wrapper.get('[data-testid="new-session"]').trigger('click')
    await flushPromises()

    expect(mockCreateSession).toHaveBeenCalledWith('agent-1', 'gpt-5')
    expect(mockSelectSession).toHaveBeenCalledWith('session-new')
    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container',
      params: { containerId: 'session-new' },
    })
  })

  it('switches to agent tab, opens editor, and can switch back to sessions', async () => {
    const wrapper = mountWorkspace()

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

  it('renders brand area with traffic-lights safe zone in sidebar top bar', async () => {
    const wrapper = mountWorkspace()
    await flushPromises()

    const brand = wrapper.get('[data-testid="workspace-brand"]')
    const safeZone = wrapper.get('[data-testid="workspace-traffic-safe-zone"]')

    expect(brand.text()).toContain('RestFlow')
    expect(safeZone.classes()).toContain('w-[5rem]')
  })

  it('navigates to canonical run route from the session list', async () => {
    const wrapper = mountWorkspace()
    await flushPromises()

    await wrapper.get('[data-testid="select-run"]').trigger('click')

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-1' },
    })
  })

  it('navigates child run thread selections to the canonical root container run route', async () => {
    const wrapper = mountWorkspace()
    await flushPromises()

    wrapper.findComponent({ name: 'ChatPanel' }).vm.$emit('threadSelection', {
      id: 'child-run-run-2',
      kind: 'child_run',
      title: 'Subagent run',
      data: {
        child_run: {
          id: 'run-2',
          kind: 'subagent_run',
          container_id: 'session-1',
          root_run_id: 'run-1',
          title: 'Subagent run',
          subtitle: null,
          status: 'completed',
          updated_at: 2,
          started_at: 1,
          ended_at: 2,
          session_id: 'session-1',
          run_id: 'run-2',
          task_id: null,
          parent_run_id: 'run-1',
          agent_id: 'agent-1',
          source_channel: 'workspace',
          source_conversation_id: null,
          effective_model: 'gpt-5',
          provider: null,
          event_count: 2,
        },
      },
    })
    await flushPromises()

    expect(mockRouterPush).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-2' },
    })
  })

  it('redirects canonical background container routes to their latest run', async () => {
    mockRoute.name = 'workspace-container'
    mockRoute.params = { containerId: 'task-1' }
    mockBackgroundStore.agents = [
      {
        id: 'task-1',
        chat_session_id: 'session-1',
      },
    ]
    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'task-1',
        kind: 'background_task',
        title: 'Digest',
        subtitle: null,
        updated_at: 1,
        status: 'completed',
        session_count: 1,
        latest_session_id: 'session-1',
        latest_run_id: 'run-1',
        agent_id: 'agent-1',
        source_channel: null,
        source_conversation_id: null,
      },
    ])
    mockListExecutionSessions.mockResolvedValue([
      {
        id: 'run-summary-1',
        kind: 'background_run',
        container_id: 'task-1',
        title: 'Run #1',
        subtitle: null,
        status: 'completed',
        updated_at: 10,
        started_at: 1,
        ended_at: 2,
        session_id: 'session-1',
        run_id: 'run-1',
        task_id: 'task-1',
        parent_run_id: null,
        agent_id: 'agent-1',
        source_channel: null,
        source_conversation_id: null,
        effective_model: 'gpt-5',
        provider: null,
        event_count: 2,
      },
    ])

    mountWorkspace()
    await flushPromises()

    expect(mockRouterReplace).toHaveBeenCalledWith({
      name: 'workspace-container-run',
      params: { containerId: 'task-1', runId: 'run-1' },
    })
  })

  it('shows an explicit empty state for containers without runs or sessions', async () => {
    mockRoute.name = 'workspace-container'
    mockRoute.params = { containerId: 'empty-container' }
    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'empty-container',
        kind: 'workspace',
        title: 'Empty Workspace Container',
        subtitle: null,
        updated_at: 1,
        status: 'pending',
        session_count: 0,
        latest_session_id: null,
        latest_run_id: null,
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
      },
    ])

    const wrapper = mountWorkspace()
    await flushPromises()

    expect(mockSelectSession).toHaveBeenCalledWith(null)
    expect(wrapper.find('[data-testid="workspace-container-empty-state"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="chat-panel"]').exists()).toBe(false)
  })

  it('shows a not-found state for unknown canonical containers', async () => {
    mockRoute.name = 'workspace-container'
    mockRoute.params = { containerId: 'missing-container' }
    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'session-1',
        kind: 'workspace',
        title: 'Workspace Session',
        subtitle: null,
        updated_at: 1,
        status: 'completed',
        session_count: 1,
        latest_session_id: 'session-1',
        latest_run_id: 'run-1',
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
      },
    ])

    const wrapper = mountWorkspace()
    await flushPromises()

    expect(mockSelectSession).toHaveBeenCalledWith(null)
    expect(wrapper.find('[data-testid="workspace-container-not-found-state"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="workspace-container-empty-state"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="chat-panel"]').exists()).toBe(false)
  })

  it('clears stale content and falls back to the container route when a canonical run is missing', async () => {
    mockRoute.name = 'workspace-container-run'
    mockRoute.params = { containerId: 'session-1', runId: 'missing-run' }
    mockGetExecutionRunThread.mockRejectedValueOnce(
      new BackendError({
        code: 404,
        message: 'ExecutionThread not found',
      } as any),
    )

    mountWorkspace()
    await flushPromises()

    expect(mockSelectSession).toHaveBeenCalledWith(null)
    expect(mockRouterReplace).toHaveBeenCalledWith({
      name: 'workspace-container',
      params: { containerId: 'session-1' },
    })
  })

  it('allows resizing the left sidebar width with drag constraints', async () => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      writable: true,
      value: 1000,
    })
    const wrapper = mountWorkspace()
    await flushPromises()

    const sidebar = wrapper.get('[data-testid="workspace-sidebar"]')
    const resizer = wrapper.get('[data-testid="workspace-sidebar-resizer"]')

    expect(sidebar.attributes('style')).toContain('width: 20.00%')

    await resizer.trigger('mousedown', { clientX: 200 })
    window.dispatchEvent(new MouseEvent('mousemove', { clientX: 320 }))
    await flushPromises()

    expect(sidebar.attributes('style')).toContain('width: 32.00%')

    window.dispatchEvent(new MouseEvent('mousemove', { clientX: 999 }))
    await flushPromises()

    expect(sidebar.attributes('style')).toContain('width: 34.00%')

    window.dispatchEvent(new MouseEvent('mouseup'))
  })
})
