import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, computed } from 'vue'
import EditorPanel from '../EditorPanel.vue'
import type { TerminalSession } from '@/types/generated/TerminalSession'
import type { EditorTab } from '@/composables/editor/useEditorTabs'

// Create reactive state for mocking
const mockTabs = ref<EditorTab[]>([])
const mockActiveTabId = ref<string | null>(null)
const mockActiveTab = computed(
  () => mockTabs.value.find((t) => t.id === mockActiveTabId.value) || null,
)

// Mock the composables
vi.mock('@/composables/editor/useEditorTabs', () => ({
  useEditorTabs: () => ({
    tabs: mockTabs,
    activeTabId: mockActiveTabId,
    activeTab: mockActiveTab,
    openTerminal: vi.fn(),
    closeTab: vi.fn(),
    switchTab: vi.fn((id: string) => {
      mockActiveTabId.value = id
    }),
    updateTabData: vi.fn(),
  }),
}))

vi.mock('@/composables/editor/useTerminalSessions', () => ({
  useTerminalSessions: () => ({
    createSession: vi.fn(() =>
      Promise.resolve({
        id: 'new-terminal',
        name: 'Terminal 1',
        status: 'running',
        created_at: Date.now(),
        history: null,
        stopped_at: null,
        working_directory: null,
        startup_command: null,
      }),
    ),
  }),
}))

// Mock child components to avoid complex dependencies
vi.mock('../TabBar.vue', () => ({
  default: {
    name: 'TabBar',
    template: '<div data-testid="tab-bar"><slot /></div>',
    props: ['tabs', 'activeTabId'],
  },
}))

vi.mock('../TerminalView.vue', () => ({
  default: {
    name: 'TerminalView',
    template:
      '<div data-testid="terminal-view" :data-session-id="session?.id">Terminal: {{ session?.name }}</div>',
    props: ['tabId', 'session'],
  },
}))

vi.mock('@/components/workspace/SkillEditor.vue', () => ({
  default: {
    name: 'SkillEditor',
    template: '<div data-testid="skill-editor">SkillEditor</div>',
    props: ['skill', 'showHeader'],
  },
}))

vi.mock('@/components/workspace/AgentEditor.vue', () => ({
  default: {
    name: 'AgentEditor',
    template: '<div data-testid="agent-editor">AgentEditor</div>',
    props: ['agent', 'showHeader'],
  },
}))

// Helper to create mock terminal session
const createMockSession = (id: string, name: string): TerminalSession => ({
  id,
  name,
  status: 'running',
  created_at: Date.now(),
  history: null,
  stopped_at: null,
  working_directory: null,
  startup_command: null,
})

// Helper to create mock terminal tab
const createTerminalTab = (session: TerminalSession): EditorTab => ({
  id: session.id,
  type: 'terminal',
  name: session.name,
  data: session,
})

describe('EditorPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockTabs.value = []
    mockActiveTabId.value = null
  })

  describe('TabBar visibility', () => {
    it('should always show TabBar even when no tabs are open', async () => {
      const wrapper = mount(EditorPanel)
      await flushPromises()

      // TabBar should be visible
      expect(wrapper.find('[data-testid="tab-bar"]').exists()).toBe(true)
    })

    it('should show TabBar when tabs are open', async () => {
      const session = createMockSession('terminal-1', 'Terminal 1')
      mockTabs.value = [createTerminalTab(session)]
      mockActiveTabId.value = 'terminal-1'

      const wrapper = mount(EditorPanel)
      await flushPromises()

      expect(wrapper.find('[data-testid="tab-bar"]').exists()).toBe(true)
    })
  })

  describe('TerminalView switching with key', () => {
    it('should render TerminalView with correct session when active', async () => {
      const session1 = createMockSession('terminal-1', 'Terminal 1')
      mockTabs.value = [createTerminalTab(session1)]
      mockActiveTabId.value = 'terminal-1'

      const wrapper = mount(EditorPanel)
      await flushPromises()

      const terminalView = wrapper.find('[data-testid="terminal-view"]')
      expect(terminalView.exists()).toBe(true)
      expect(terminalView.attributes('data-session-id')).toBe('terminal-1')
    })

    it('should use key to force remount when switching terminals', async () => {
      const session1 = createMockSession('terminal-1', 'Terminal 1')
      const session2 = createMockSession('terminal-2', 'Terminal 2')
      mockTabs.value = [createTerminalTab(session1), createTerminalTab(session2)]
      mockActiveTabId.value = 'terminal-1'

      const wrapper = mount(EditorPanel)
      await flushPromises()

      // First terminal should be active
      let terminalView = wrapper.find('[data-testid="terminal-view"]')
      expect(terminalView.attributes('data-session-id')).toBe('terminal-1')

      // Switch to second terminal
      mockActiveTabId.value = 'terminal-2'
      await flushPromises()

      // Second terminal should be active
      terminalView = wrapper.find('[data-testid="terminal-view"]')
      expect(terminalView.attributes('data-session-id')).toBe('terminal-2')

      // Switch back to first terminal
      mockActiveTabId.value = 'terminal-1'
      await flushPromises()

      // First terminal should be active again
      terminalView = wrapper.find('[data-testid="terminal-view"]')
      expect(terminalView.attributes('data-session-id')).toBe('terminal-1')
    })

    it('should show different terminal content when switching between tabs', async () => {
      const session1 = createMockSession('terminal-1', 'Dev Terminal')
      const session2 = createMockSession('terminal-2', 'Prod Terminal')
      mockTabs.value = [createTerminalTab(session1), createTerminalTab(session2)]
      mockActiveTabId.value = 'terminal-1'

      const wrapper = mount(EditorPanel)
      await flushPromises()

      // Check first terminal name
      expect(wrapper.text()).toContain('Dev Terminal')

      // Switch to second terminal
      mockActiveTabId.value = 'terminal-2'
      await flushPromises()

      // Check second terminal name
      expect(wrapper.text()).toContain('Prod Terminal')
    })
  })

  describe('empty state', () => {
    it('should show empty state when no tabs are open', async () => {
      const wrapper = mount(EditorPanel)
      await flushPromises()

      expect(wrapper.text()).toContain('No tabs open')
    })

    it('should hide empty state when tabs are open', async () => {
      const session = createMockSession('terminal-1', 'Terminal 1')
      mockTabs.value = [createTerminalTab(session)]
      mockActiveTabId.value = 'terminal-1'

      const wrapper = mount(EditorPanel)
      await flushPromises()

      expect(wrapper.text()).not.toContain('No tabs open')
    })
  })
})
