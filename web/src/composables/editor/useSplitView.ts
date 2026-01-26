/**
 * Split View State Management
 *
 * Manages the state of the split view panel where tabs can be pinned.
 * State is persisted to localStorage.
 *
 * Implementation Note:
 * We use a Pin button (click) instead of drag-and-drop to pin tabs because
 * Tauri's WebView has `dragDropEnabled` enabled by default, which intercepts
 * HTML5 drag events to support file drag-drop from the system (e.g., Finder).
 * This causes `dragover` and `drop` events to never fire, while only
 * `dragstart` and `dragend` work. Since we need to preserve system file
 * drag-drop functionality, a Pin button provides a reliable alternative.
 *
 * References:
 * - https://github.com/tauri-apps/tauri/issues/8581
 * - https://github.com/tauri-apps/tauri/issues/6695
 */

import { ref, computed, watch } from 'vue'

const STORAGE_KEY = 'restflow-split-view'

// All widths are ratios of window width for responsive behavior
const DEFAULT_WIDTH_RATIO = 0.3 // 30% of window width
const MIN_WIDTH_RATIO = 0.2 // 20% of window width
const MAX_WIDTH_RATIO = 0.7 // 70% of window width

// Calculate actual pixel values from ratios
function getDefaultWidth() {
  return Math.floor(window.innerWidth * DEFAULT_WIDTH_RATIO)
}

function getMinWidth() {
  return Math.floor(window.innerWidth * MIN_WIDTH_RATIO)
}

function getMaxWidth() {
  return Math.floor(window.innerWidth * MAX_WIDTH_RATIO)
}

interface SplitViewState {
  enabled: boolean
  pinnedTabId: string | null
  width: number
}

// Global singleton state
const state = ref<SplitViewState>({
  enabled: false,
  pinnedTabId: null,
  width: getDefaultWidth(),
})

// Load from localStorage on module initialization
const saved = localStorage.getItem(STORAGE_KEY)
if (saved) {
  try {
    const parsed = JSON.parse(saved) as Partial<SplitViewState>
    state.value = {
      enabled: parsed.enabled ?? false,
      pinnedTabId: parsed.pinnedTabId ?? null,
      width: parsed.width ?? getDefaultWidth(),
    }
  } catch {
    console.warn('Failed to parse split view state from localStorage')
  }
}

// Persist changes to localStorage
watch(
  state,
  (newState) => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(newState))
  },
  { deep: true },
)

export function useSplitView() {
  const isEnabled = computed(() => state.value.enabled)
  const pinnedTabId = computed(() => state.value.pinnedTabId)
  const splitWidth = computed(() => state.value.width)

  /**
   * Pin a tab to the split view
   */
  function pinTab(tabId: string) {
    state.value.enabled = true
    state.value.pinnedTabId = tabId
  }

  /**
   * Unpin and close split view
   */
  function unpinTab() {
    state.value.enabled = false
    state.value.pinnedTabId = null
  }

  /**
   * Toggle split view for a tab
   */
  function togglePin(tabId: string) {
    if (state.value.pinnedTabId === tabId) {
      unpinTab()
    } else {
      pinTab(tabId)
    }
  }

  /**
   * Resize split view width
   * Min/max widths are dynamically calculated based on window width
   */
  function setSplitWidth(width: number) {
    state.value.width = Math.max(getMinWidth(), Math.min(getMaxWidth(), width))
  }

  /**
   * Check if a tab is pinned
   */
  function isPinned(tabId: string) {
    return state.value.pinnedTabId === tabId
  }

  /**
   * Handle when a pinned tab is closed externally
   */
  function handleTabClosed(tabId: string) {
    if (state.value.pinnedTabId === tabId) {
      unpinTab()
    }
  }

  return {
    isEnabled,
    pinnedTabId,
    splitWidth,
    pinTab,
    unpinTab,
    togglePin,
    setSplitWidth,
    isPinned,
    handleTabClosed,
    // Export ratio constants and helper functions
    DEFAULT_WIDTH_RATIO,
    MIN_WIDTH_RATIO,
    MAX_WIDTH_RATIO,
    getDefaultWidth,
    getMinWidth,
    getMaxWidth,
  }
}
