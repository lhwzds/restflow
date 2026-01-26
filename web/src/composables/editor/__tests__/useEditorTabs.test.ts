import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// Mock data
const mockSkill: Skill = {
  id: 'skill-1',
  name: 'Test Skill',
  content: '# Test',
  tags: [],
  description: null,
  created_at: 1000,
  updated_at: 2000,
}

const mockAgent: StoredAgent = {
  id: 'agent-1',
  name: 'Test Agent',
  agent: { model: 'gpt-5' },
  created_at: 1000,
  updated_at: 2000,
}

const mockSession: TerminalSession = {
  id: 'terminal-1',
  name: 'Terminal 1',
  status: 'running',
  created_at: 1000,
  history: null,
  stopped_at: null,
  working_directory: null,
  startup_command: null,
}

describe('useEditorTabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with no tabs and showBrowser false', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, hasOpenTabs, showBrowser, closeAllTabs } = useEditorTabs()

      // Clean up any existing state
      closeAllTabs()

      expect(tabs.value).toEqual([])
      expect(hasOpenTabs.value).toBe(false)
      expect(showBrowser.value).toBe(false)
    })
  })

  describe('openSkill', () => {
    it('should open a skill in a new tab and exit browse mode', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, showBrowser, openSkill, enterBrowseMode, closeAllTabs } =
        useEditorTabs()

      closeAllTabs()
      enterBrowseMode()
      expect(showBrowser.value).toBe(true)

      openSkill(mockSkill)

      expect(tabs.value).toHaveLength(1)
      expect(activeTabId.value).toBe('skill-1')
      expect(showBrowser.value).toBe(false)

      closeAllTabs()
    })

    it('should focus existing tab if skill is already open', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, openSkill, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)
      expect(tabs.value).toHaveLength(1)

      // Open another skill
      openSkill({ ...mockSkill, id: 'skill-2', name: 'Skill 2' })
      expect(tabs.value).toHaveLength(2)
      expect(activeTabId.value).toBe('skill-2')

      // Open first skill again - should focus, not add
      openSkill(mockSkill)
      expect(tabs.value).toHaveLength(2)
      expect(activeTabId.value).toBe('skill-1')

      closeAllTabs()
    })
  })

  describe('openAgent', () => {
    it('should open an agent in a new tab and exit browse mode', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, showBrowser, openAgent, enterBrowseMode, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      enterBrowseMode()

      openAgent(mockAgent)

      expect(tabs.value).toHaveLength(1)
      expect(tabs.value[0]!.type).toBe('agent')
      expect(showBrowser.value).toBe(false)

      closeAllTabs()
    })
  })

  describe('openTerminal', () => {
    it('should open a terminal in a new tab and exit browse mode', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, showBrowser, openTerminal, enterBrowseMode, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      enterBrowseMode()

      openTerminal(mockSession)

      expect(tabs.value).toHaveLength(1)
      expect(tabs.value[0]!.type).toBe('terminal')
      expect(showBrowser.value).toBe(false)

      closeAllTabs()
    })

    it('should open multiple terminals and switch between them', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, activeTab, openTerminal, switchTab, closeAllTabs } =
        useEditorTabs()

      closeAllTabs()

      const session1: TerminalSession = { ...mockSession, id: 'terminal-1', name: 'Terminal 1' }
      const session2: TerminalSession = { ...mockSession, id: 'terminal-2', name: 'Terminal 2' }

      // Open first terminal
      openTerminal(session1)
      expect(tabs.value).toHaveLength(1)
      expect(activeTabId.value).toBe('terminal-1')
      expect(activeTab.value?.data).toEqual(session1)

      // Open second terminal
      openTerminal(session2)
      expect(tabs.value).toHaveLength(2)
      expect(activeTabId.value).toBe('terminal-2')
      expect(activeTab.value?.data).toEqual(session2)

      // Switch back to first terminal
      switchTab('terminal-1')
      expect(activeTabId.value).toBe('terminal-1')
      expect(activeTab.value?.data).toEqual(session1)

      // Switch to second terminal
      switchTab('terminal-2')
      expect(activeTabId.value).toBe('terminal-2')
      expect(activeTab.value?.data).toEqual(session2)

      closeAllTabs()
    })
  })

  describe('switchTab', () => {
    it('should switch to a tab and exit browse mode', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const {
        activeTabId,
        showBrowser,
        openSkill,
        openAgent,
        switchTab,
        enterBrowseMode,
        closeAllTabs,
      } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)
      openAgent(mockAgent)

      enterBrowseMode()
      expect(showBrowser.value).toBe(true)

      switchTab('skill-1')
      expect(activeTabId.value).toBe('skill-1')
      expect(showBrowser.value).toBe(false)

      closeAllTabs()
    })
  })

  describe('closeTab', () => {
    it('should close a tab and activate another', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, openSkill, closeTab, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)
      openSkill({ ...mockSkill, id: 'skill-2', name: 'Skill 2' })
      expect(tabs.value).toHaveLength(2)
      expect(activeTabId.value).toBe('skill-2')

      closeTab('skill-2')
      expect(tabs.value).toHaveLength(1)
      expect(activeTabId.value).toBe('skill-1')

      closeAllTabs()
    })

    it('should clear activeTabId when closing last tab', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, openSkill, closeTab, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)
      expect(tabs.value).toHaveLength(1)

      closeTab('skill-1')
      expect(tabs.value).toHaveLength(0)
      expect(activeTabId.value).toBeNull()
    })
  })

  describe('enterBrowseMode / exitBrowseMode', () => {
    it('should toggle showBrowser state', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { showBrowser, enterBrowseMode, exitBrowseMode, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      expect(showBrowser.value).toBe(false)

      enterBrowseMode()
      expect(showBrowser.value).toBe(true)

      exitBrowseMode()
      expect(showBrowser.value).toBe(false)
    })
  })

  describe('setTabDirty', () => {
    it('should mark a tab as dirty', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, openSkill, setTabDirty, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)
      expect(tabs.value[0]!.isDirty).toBeUndefined()

      setTabDirty('skill-1', true)
      expect(tabs.value[0]!.isDirty).toBe(true)

      setTabDirty('skill-1', false)
      expect(tabs.value[0]!.isDirty).toBe(false)

      closeAllTabs()
    })
  })

  describe('updateTabData', () => {
    it('should update tab data and name', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, openSkill, updateTabData, closeAllTabs } = useEditorTabs()

      closeAllTabs()
      openSkill(mockSkill)

      const updatedSkill = { ...mockSkill, name: 'Updated Skill' }
      updateTabData('skill-1', updatedSkill)

      expect(tabs.value[0]!.data).toEqual(updatedSkill)
      expect(tabs.value[0]!.name).toBe('Updated Skill.md')
      expect(tabs.value[0]!.isDirty).toBe(false)

      closeAllTabs()
    })
  })

  describe('closeAllTabs', () => {
    it('should close all tabs and reset state', async () => {
      const { useEditorTabs } = await import('../useEditorTabs')
      const { tabs, activeTabId, openSkill, openAgent, closeAllTabs } = useEditorTabs()

      openSkill(mockSkill)
      openAgent(mockAgent)
      expect(tabs.value).toHaveLength(2)

      closeAllTabs()
      expect(tabs.value).toHaveLength(0)
      expect(activeTabId.value).toBeNull()
    })
  })
})
