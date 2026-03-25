import { computed, ref } from 'vue'
import type { StreamStep } from './useChatStream'
import type { ThreadSelection } from '@/components/chat/threadItems'

export type ToolPanelType =
  | 'canvas'
  | 'terminal'
  | 'http'
  | 'file'
  | 'search'
  | 'python'
  | 'web'
  | 'browser'
  | 'notification'
  | 'generic'

export interface ToolPanelHistoryEntry {
  toolId: string
  toolName: string
  panelType: ToolPanelType
  title: string
  data: Record<string, unknown>
  step?: StreamStep
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
  step?: StreamStep
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
  browser: 'browser',
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
    step: undefined,
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

function stringifyUnknown(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, current) => (typeof current === 'bigint' ? current.toString() : current),
    2,
  )
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function normalizeStepStatus(status: unknown): StreamStep['status'] {
  switch (status) {
    case 'completed':
    case 'failed':
    case 'pending':
    case 'running':
      return status
    default:
      return 'completed'
  }
}

function toPanelType(toolName: string): ToolPanelType {
  return TOOL_PANEL_MAP[toolName] ?? 'generic'
}

function isPersistedExecutionStepData(data: Record<string, unknown>): boolean {
  return data.persisted_execution_step === true
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
  if (panelType === 'browser') {
    const action = typeof data.action === 'string' ? data.action : ''
    return action ? `browser: ${action}` : 'browser'
  }
  return toolName || 'Tool Result'
}

function toStepEntryId(step: StreamStep, data: Record<string, unknown>): string {
  if (step.toolId) return step.toolId

  if (isPersistedExecutionStepData(data)) {
    const messageId =
      typeof data.message_id === 'string' && data.message_id.length > 0 ? data.message_id : 'step'
    const stepIndex =
      typeof data.step_index === 'number' && Number.isFinite(data.step_index)
        ? data.step_index
        : 'unknown'
    return `persisted-${messageId}-${stepIndex}`
  }

  return `${step.type || 'step'}-${step.name || 'unknown'}-${Date.now()}`
}

function toPersistedStepTitle(step: StreamStep): string {
  if (step.type === 'tool_call') {
    return `${step.name || 'Tool'} details`
  }
  return `${step.type || 'step'}: ${step.name || 'details'}`
}

function toCanonicalToolEventStep(selection: ThreadSelection): StreamStep | null {
  if (selection.kind !== 'event') return null

  const event = isRecord(selection.data.event) ? selection.data.event : null
  if (!event || event.category !== 'tool_call') return null

  const toolCall = isRecord(event.tool_call) ? event.tool_call : null
  const toolName =
    typeof toolCall?.tool_name === 'string' && toolCall.tool_name.length > 0
      ? toolCall.tool_name
      : selection.toolName
  if (!toolName) return null

  const inputPayload =
    toolCall?.input ??
    {
      input_summary:
        typeof toolCall?.input_summary === 'string' ? toolCall.input_summary : null,
      tool_call_id:
        typeof toolCall?.tool_call_id === 'string' ? toolCall.tool_call_id : null,
    }
  const outputPayload =
    toolCall?.output ??
    {
      output_ref:
        typeof toolCall?.output_ref === 'string' ? toolCall.output_ref : null,
      error: typeof toolCall?.error === 'string' ? toolCall.error : null,
      success: typeof toolCall?.success === 'boolean' ? toolCall.success : null,
      duration_ms:
        typeof toolCall?.duration_ms === 'number' || typeof toolCall?.duration_ms === 'bigint'
          ? toolCall.duration_ms
          : null,
    }

  return {
    type: 'tool_call',
    name: toolName,
    displayName: toolName,
    status: normalizeStepStatus(toolCall?.phase),
    toolId:
      (typeof toolCall?.tool_call_id === 'string' && toolCall.tool_call_id.length > 0
        ? toolCall.tool_call_id
        : selection.id),
    arguments:
      typeof inputPayload === 'string' ? inputPayload : stringifyUnknown(inputPayload),
    result:
      typeof outputPayload === 'string' ? outputPayload : stringifyUnknown(outputPayload),
  }
}

function selectionStatus(selection: ThreadSelection): 'completed' | 'failed' {
  const rawStatus =
    typeof selection.data.status === 'string'
      ? selection.data.status
      : typeof selection.data.event === 'object' &&
          selection.data.event &&
          'lifecycle' in selection.data.event &&
          selection.data.event.lifecycle &&
          typeof selection.data.event.lifecycle === 'object' &&
          'status' in selection.data.event.lifecycle &&
          typeof selection.data.event.lifecycle.status === 'string'
        ? selection.data.event.lifecycle.status
        : 'completed'

  return rawStatus === 'failed' ? 'failed' : 'completed'
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
    state.value.step = entry.step
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
    if (!step.name) return

    const parsedResult = parseResult(step.result)
    const parsedArgs = parseResult(step.arguments)
    // Merge arguments into data so toTitle can access fields like `url`
    const data = { ...parsedArgs, ...parsedResult }
    const persistedStep = isPersistedExecutionStepData(data)
    const toolId = toStepEntryId(step, data)
    const panelType =
      persistedStep || step.type !== 'tool_call' ? 'generic' : toPanelType(step.name)
    const title = persistedStep
      ? toPersistedStepTitle(step)
      : panelType === 'generic' && step.type !== 'tool_call'
        ? toPersistedStepTitle(step)
        : toTitle(step.name, panelType, data)

    appendEntry({
      toolId,
      toolName: step.name,
      panelType,
      title,
      data,
      step,
      timestamp: Date.now(),
      status: step.status === 'failed' ? 'failed' : 'completed',
    })
  }

  function handleShowPanelResult(resultJson: string) {
    const data = parseResult(resultJson)
    const toolId = `legacy-show-panel-${Date.now()}`
    appendEntry({
      toolId,
      toolName: 'show_panel',
      panelType: 'canvas',
      title: toTitle('show_panel', 'canvas', data),
      data,
      step: {
        type: 'tool_call',
        name: 'show_panel',
        status: 'completed',
        toolId,
        result: resultJson,
      },
      timestamp: Date.now(),
      status: 'completed',
    })
  }

  function handleThreadSelection(selection: ThreadSelection) {
    if (selection.kind === 'step' && selection.step) {
      handleToolResult(selection.step)
      return
    }

    const canonicalToolEventStep = toCanonicalToolEventStep(selection)
    if (canonicalToolEventStep) {
      handleToolResult(canonicalToolEventStep)
      return
    }

    appendEntry({
      toolId: selection.id,
      toolName: selection.toolName ?? selection.title,
      panelType: 'generic',
      title: selection.title,
      data: selection.data,
      step: undefined,
      timestamp: Date.now(),
      status: selectionStatus(selection),
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
    handleThreadSelection,
    handleShowPanelResult,
    showPanel,
    navigateHistory,
    closePanel,
    clearHistory,
  }
}

export type UseToolPanelReturn = ReturnType<typeof useToolPanel>
