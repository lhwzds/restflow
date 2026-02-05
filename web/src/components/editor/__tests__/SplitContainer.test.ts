import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref } from 'vue'
import SplitContainer from '../SplitContainer.vue'
import type { EditorTab } from '@/composables/editor/useEditorTabs'
import type { Skill } from '@/types/generated/Skill'

// Mock state
const mockIsEnabled = ref(false)
const mockPinnedTabId = ref<string | null>(null)
const mockSplitWidth = ref(400)
const mockTabs = ref<EditorTab[]>([])

const mockUnpinTab = vi.fn()
const mockSetSplitWidth = vi.fn((width: number) => {
  // Simulate the dynamic max width calculation
  const maxWidth = Math.floor(window.innerWidth * 0.7)
  mockSplitWidth.value = Math.max(300, Math.min(maxWidth, width))
})
const mockCloseTab = vi.fn()

// Mock composables
vi.mock('@/composables/editor/useSplitView', () => ({
  useSplitView: () => ({
    isEnabled: mockIsEnabled,
    pinnedTabId: mockPinnedTabId,
    splitWidth: mockSplitWidth,
    unpinTab: mockUnpinTab,
    setSplitWidth: mockSetSplitWidth,
  }),
}))

vi.mock('@/composables/editor/useEditorTabs', () => ({
  useEditorTabs: () => ({
    tabs: mockTabs,
    closeTab: mockCloseTab,
  }),
}))

// Mock child components
vi.mock('../TerminalView.vue', () => ({
  default: {
    name: 'TerminalView',
    template: '<div data-testid="terminal-view">Terminal</div>',
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

// Helper to create mock skill tab
const createSkillTab = (): EditorTab => ({
  id: 'skill-1',
  type: 'skill',
  name: 'Test Skill.md',
  data: {
    id: 'skill-1',
    name: 'Test Skill',
    content: '# Test',
    tags: [],
    description: null,
    folder_path: null,
    gating: null,
    version: null,
    author: null,
    license: null,
    content_hash: null,
    storage_mode: 'DatabaseOnly',
    is_synced: false,
    created_at: 1000,
    updated_at: 2000,
  } as Skill,
})

// Helper to create mock terminal tab
const createTerminalTab = (): EditorTab => ({
  id: 'terminal-1',
  type: 'terminal',
  name: 'Terminal 1',
  data: {
    id: 'terminal-1',
    name: 'Terminal 1',
    status: 'running',
    created_at: 1000,
    history: null,
    stopped_at: null,
    working_directory: null,
    startup_command: null,
  },
})

describe('SplitContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockIsEnabled.value = false
    mockPinnedTabId.value = null
    mockSplitWidth.value = 400
    mockTabs.value = []
  })

  describe('visibility', () => {
    it('should not render when split view is disabled', async () => {
      mockIsEnabled.value = false

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.find('[data-testid="split-view-panel"]').exists()).toBe(false)
    })

    it('should not render when no tab is pinned', async () => {
      mockIsEnabled.value = true
      mockPinnedTabId.value = null

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.find('[data-testid="split-view-panel"]').exists()).toBe(false)
    })

    it('should render when enabled and tab is pinned', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.find('[data-testid="split-view-panel"]').exists()).toBe(true)
    })
  })

  describe('resize handle', () => {
    it('should have a resize handle with expanded hit area', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      // Check for resize handle structure
      const resizeContainer = wrapper.find('.relative.w-0')
      expect(resizeContainer.exists()).toBe(true)

      // Check for expanded hit area (w-3 = 12px)
      const hitArea = wrapper.find('.w-3.cursor-ew-resize')
      expect(hitArea.exists()).toBe(true)

      // Check for visual line (w-px = 1px)
      const visualLine = wrapper.find('.w-px.bg-border')
      expect(visualLine.exists()).toBe(true)
    })

    it('should call setSplitWidth when dragging', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      const hitArea = wrapper.find('.w-3.cursor-ew-resize')

      // Start dragging
      await hitArea.trigger('mousedown', { clientX: 500 })

      // Simulate mouse move (moving left increases width)
      const mouseMoveEvent = new MouseEvent('mousemove', { clientX: 450 })
      document.dispatchEvent(mouseMoveEvent)

      expect(mockSetSplitWidth).toHaveBeenCalled()

      // Stop dragging
      const mouseUpEvent = new MouseEvent('mouseup')
      document.dispatchEvent(mouseUpEvent)
    })

    it('should set cursor style during drag', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      const hitArea = wrapper.find('.w-3.cursor-ew-resize')

      // Start dragging
      await hitArea.trigger('mousedown', { clientX: 500 })
      expect(document.body.style.cursor).toBe('ew-resize')
      expect(document.body.style.userSelect).toBe('none')

      // Stop dragging
      const mouseUpEvent = new MouseEvent('mouseup')
      document.dispatchEvent(mouseUpEvent)

      expect(document.body.style.cursor).toBe('')
      expect(document.body.style.userSelect).toBe('')
    })
  })

  describe('header actions', () => {
    it('should call unpinTab when clicking unpin button', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      const unpinButton = wrapper.find('button[title="Unpin"]')
      await unpinButton.trigger('click')

      expect(mockUnpinTab).toHaveBeenCalled()
    })

    it('should call closeTab and unpinTab when clicking close button', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      const closeButton = wrapper.find('button[title="Close"]')
      await closeButton.trigger('click')

      expect(mockCloseTab).toHaveBeenCalledWith('skill-1')
      expect(mockUnpinTab).toHaveBeenCalled()
    })
  })

  describe('content rendering', () => {
    it('should render SkillEditor for skill tab', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.find('[data-testid="skill-editor"]').exists()).toBe(true)
    })

    it('should render TerminalView for terminal tab', async () => {
      const tab = createTerminalTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'terminal-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.find('[data-testid="terminal-view"]').exists()).toBe(true)
    })

    it('should display tab name in header', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.text()).toContain('Test Skill.md')
    })

    it('should show dirty indicator when tab is dirty', async () => {
      const tab = { ...createSkillTab(), isDirty: true }
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]

      const wrapper = mount(SplitContainer)
      await flushPromises()

      expect(wrapper.text()).toContain('*')
    })
  })

  describe('width styling', () => {
    it('should apply splitWidth to panel style', async () => {
      const tab = createSkillTab()
      mockIsEnabled.value = true
      mockPinnedTabId.value = 'skill-1'
      mockTabs.value = [tab]
      mockSplitWidth.value = 500

      const wrapper = mount(SplitContainer)
      await flushPromises()

      const panel = wrapper.find('[data-testid="split-view-panel"]')
      expect(panel.attributes('style')).toContain('width: 500px')
    })
  })
})
