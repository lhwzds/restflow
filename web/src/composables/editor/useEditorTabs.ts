import { ref, computed } from 'vue'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TerminalSession } from '@/types/generated/TerminalSession'

export type TabType = 'skill' | 'agent' | 'terminal'

export interface EditorTab {
  id: string
  type: TabType
  name: string
  data?: Skill | StoredAgent | TerminalSession
  isDirty?: boolean
}

// Global state for editor tabs
const tabs = ref<EditorTab[]>([])
const activeTabId = ref<string | null>(null)
// When true, show file browser even when tabs are open
const showBrowser = ref(false)

export function useEditorTabs() {
  const activeTab = computed(() => {
    if (!activeTabId.value) return null
    return tabs.value.find((t) => t.id === activeTabId.value) || null
  })

  const hasOpenTabs = computed(() => tabs.value.length > 0)

  // Show the file browser (hide editor content)
  function enterBrowseMode() {
    showBrowser.value = true
  }

  // Show the editor content (hide file browser)
  function exitBrowseMode() {
    showBrowser.value = false
  }

  // Open a skill in a new tab or focus existing
  function openSkill(skill: Skill) {
    const existingTab = tabs.value.find((t) => t.type === 'skill' && t.id === skill.id)
    if (existingTab) {
      activeTabId.value = existingTab.id
      showBrowser.value = false
      return
    }

    const tab: EditorTab = {
      id: skill.id,
      type: 'skill',
      name: `${skill.name}.md`,
      data: skill,
    }
    tabs.value.push(tab)
    activeTabId.value = tab.id
    showBrowser.value = false
  }

  // Open an agent in a new tab or focus existing
  function openAgent(agent: StoredAgent) {
    const existingTab = tabs.value.find((t) => t.type === 'agent' && t.id === agent.id)
    if (existingTab) {
      activeTabId.value = existingTab.id
      showBrowser.value = false
      return
    }

    const tab: EditorTab = {
      id: agent.id,
      type: 'agent',
      name: agent.name,
      data: agent,
    }
    tabs.value.push(tab)
    activeTabId.value = tab.id
    showBrowser.value = false
  }

  // Open a terminal session in a new tab or focus existing
  function openTerminal(session: TerminalSession): EditorTab {
    // Check if already open
    const existingTab = tabs.value.find((t) => t.type === 'terminal' && t.id === session.id)
    if (existingTab) {
      // Update session data (it may have changed, e.g., after restart)
      existingTab.data = session
      activeTabId.value = existingTab.id
      showBrowser.value = false
      return existingTab
    }

    // Create new tab for this session
    const tab: EditorTab = {
      id: session.id,
      type: 'terminal',
      name: session.name,
      data: session,
    }
    tabs.value.push(tab)
    activeTabId.value = tab.id
    showBrowser.value = false
    return tab
  }

  // Close a tab by id
  function closeTab(tabId: string) {
    const index = tabs.value.findIndex((t) => t.id === tabId)
    if (index === -1) return

    tabs.value.splice(index, 1)

    // If we closed the active tab, activate another one
    if (activeTabId.value === tabId) {
      if (tabs.value.length > 0) {
        // Activate the tab at the same index, or the last one
        const newIndex = Math.min(index, tabs.value.length - 1)
        const nextTab = tabs.value[newIndex]
        activeTabId.value = nextTab?.id ?? null
      } else {
        activeTabId.value = null
      }
    }
  }

  // Switch to a tab
  function switchTab(tabId: string) {
    if (tabs.value.some((t) => t.id === tabId)) {
      activeTabId.value = tabId
      showBrowser.value = false
    }
  }

  // Mark a tab as dirty (has unsaved changes)
  function setTabDirty(tabId: string, dirty: boolean) {
    const tab = tabs.value.find((t) => t.id === tabId)
    if (tab) {
      tab.isDirty = dirty
    }
  }

  // Update tab name
  function setTabName(tabId: string, name: string) {
    const tab = tabs.value.find((t) => t.id === tabId)
    if (tab) {
      tab.name = name
    }
  }

  // Update tab data (after save)
  function updateTabData(tabId: string, data: Skill | StoredAgent) {
    const tab = tabs.value.find((t) => t.id === tabId)
    if (tab) {
      tab.data = data
      tab.isDirty = false
      // Update name based on data
      if (tab.type === 'skill') {
        tab.name = `${(data as Skill).name}.md`
      } else if (tab.type === 'agent') {
        tab.name = (data as StoredAgent).name
      }
    }
  }

  // Close all tabs
  function closeAllTabs() {
    tabs.value = []
    activeTabId.value = null
  }

  return {
    tabs,
    activeTabId,
    activeTab,
    hasOpenTabs,
    showBrowser,
    enterBrowseMode,
    exitBrowseMode,
    openSkill,
    openAgent,
    openTerminal,
    closeTab,
    switchTab,
    setTabDirty,
    setTabName,
    updateTabData,
    closeAllTabs,
  }
}
