import { computed, ref } from 'vue'
import type { StreamStep } from './useChatStream'

export type ToolPanelType =
  | 'canvas'
  | 'terminal'
  | 'http'
  | 'file'
  | 'search'
  | 'python'
  | 'web'
  | 'notification'
  | 'generic'

export interface ToolPanelHistoryEntry {
  toolId: string
  toolName: string
  panelType: ToolPanelType
  title: string
  data: Record<string, unknown>
  timestamp: number
  status: 'completed' | 'failed'
}

export interface ToolPanelState {
  visible: boolean
  panelType: ToolPanelType
  toolName: string
  toolId: string
  title: string
  data: Record<string, unknown>
  history: ToolPanelHistoryEntry[]
  historyIndex: number
}

const TOOL_PANEL_MAP: Record<string, ToolPanelType> = {
  bash: 'terminal',
  process: 'terminal',
  http_request: 'http',
  http: 'http',
  file: 'file',
  patch: 'file',
  web_search: 'search',
  memory_search: 'search',
  run_python: 'python',
  python: 'python',
  web_fetch: 'web',
  jina_reader: 'web',
  send_email: 'notification',
  email: 'notification',
  telegram_send: 'notification',
  telegram: 'notification',
  show_panel: 'canvas',
}

function createInitialState(): ToolPanelState {
  return {
    visible: false,
    panelType: 'generic',
    toolName: '',
    toolId: '',
    title: '',
    data: {},
    history: [],
    historyIndex: -1,
  }
}

function parseResult(result?: string): Record<string, unknown> {
  if (!result) return {}

  try {
    const parsed = JSON.parse(result)
    if (parsed && typeof parsed === 'object') {
      return parsed as Record<string, unknown>
    }
    return { value: parsed }
  } catch {
    return { raw: result }
  }
}

function toPanelType(toolName: string): ToolPanelType {
  return TOOL_PANEL_MAP[toolName] ?? 'generic'
}

function toTitle(
  toolName: string,
  panelType: ToolPanelType,
  data: Record<string, unknown>,
): string {
  if (panelType === 'canvas' && typeof data.title === 'string' && data.title.length > 0) {
    return data.title
  }
  if (panelType === 'http' && typeof data.url === 'string') {
    return `${toolName}: ${data.url}`
  }
  return toolName || 'Tool Result'
}

export function useToolPanel() {
  const state = ref<ToolPanelState>(createInitialState())

  const activeEntry = computed<ToolPanelHistoryEntry | null>(() => {
    const history = state.value.history
    if (history.length === 0) return null

    if (state.value.historyIndex >= 0) {
      return history[state.value.historyIndex] ?? null
    }

    return history[history.length - 1] ?? null
  })

  function syncFromEntry(entry: ToolPanelHistoryEntry) {
    state.value.visible = true
    state.value.panelType = entry.panelType
    state.value.toolName = entry.toolName
    state.value.toolId = entry.toolId
    state.value.title = entry.title
    state.value.data = entry.data
  }

  function appendEntry(entry: ToolPanelHistoryEntry) {
    state.value.history.push(entry)
    state.value.historyIndex = -1
    syncFromEntry(entry)
  }

  function showPanel(entry: ToolPanelHistoryEntry) {
    const index = state.value.history.findIndex((item) => item.toolId === entry.toolId)
    state.value.historyIndex = index >= 0 ? index : -1
    syncFromEntry(entry)
  }

  function handleToolResult(step: StreamStep) {
    if (step.type !== 'tool_call' || !step.toolId || !step.name) return

    const parsed = parseResult(step.result)
    const panelType = toPanelType(step.name)
    const title = toTitle(step.name, panelType, parsed)

    appendEntry({
      toolId: step.toolId,
      toolName: step.name,
      panelType,
      title,
      data: parsed,
      timestamp: Date.now(),
      status: step.status === 'failed' ? 'failed' : 'completed',
    })
  }

  function handleShowPanelResult(resultJson: string) {
    const data = parseResult(resultJson)
    appendEntry({
      toolId: `legacy-show-panel-${Date.now()}`,
      toolName: 'show_panel',
      panelType: 'canvas',
      title: toTitle('show_panel', 'canvas', data),
      data,
      timestamp: Date.now(),
      status: 'completed',
    })
  }

  function navigateHistory(direction: 'prev' | 'next') {
    const history = state.value.history
    if (history.length === 0) return

    const latestIndex = history.length - 1
    const currentIndex = state.value.historyIndex >= 0 ? state.value.historyIndex : latestIndex
    const nextIndex = direction === 'prev' ? currentIndex - 1 : currentIndex + 1

    if (nextIndex < 0 || nextIndex > latestIndex) return

    state.value.historyIndex = nextIndex === latestIndex ? -1 : nextIndex
    const entry = history[nextIndex]
    if (entry) {
      syncFromEntry(entry)
    }
  }

  function closePanel() {
    state.value.visible = false
  }

  function clearHistory() {
    state.value = createInitialState()
  }

  const visible = computed(() => state.value.visible)
  const canNavigatePrev = computed(() => {
    const history = state.value.history
    if (history.length <= 1) return false
    const latestIndex = history.length - 1
    const currentIndex = state.value.historyIndex >= 0 ? state.value.historyIndex : latestIndex
    return currentIndex > 0
  })
  const canNavigateNext = computed(() => {
    const history = state.value.history
    if (history.length <= 1) return false
    const latestIndex = history.length - 1
    const currentIndex = state.value.historyIndex >= 0 ? state.value.historyIndex : latestIndex
    return currentIndex < latestIndex
  })

  return {
    state,
    visible,
    activeEntry,
    canNavigatePrev,
    canNavigateNext,
    handleToolResult,
    handleShowPanelResult,
    showPanel,
    navigateHistory,
    closePanel,
    clearHistory,
  }
}

export type UseToolPanelReturn = ReturnType<typeof useToolPanel>
