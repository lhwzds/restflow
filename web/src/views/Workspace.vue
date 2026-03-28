<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Container/run sidebar
 * - Center: Unified execution thread panel
 * - Right: Tool panel / inspector
 */
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Settings, Moon, Sun, Bot, MessageSquare } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import SessionList from '@/components/workspace/SessionList.vue'
import AgentList from '@/components/workspace/AgentList.vue'
import AgentEditorPanel from '@/components/workspace/AgentEditorPanel.vue'
import SettingsPanel from '@/components/settings/SettingsPanel.vue'
import ChatPanel from '@/components/chat/ChatPanel.vue'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'
import ConvertToBackgroundAgentDialog from '@/components/workspace/ConvertToBackgroundAgentDialog.vue'
import CreateAgentDialog from '@/components/workspace/CreateAgentDialog.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import { useTheme } from '@/composables/useTheme'
import { confirmDelete, useConfirm } from '@/composables/useConfirm'
import { deleteAgent as deleteAgentApi, listAgents } from '@/api/agents'
import {
  getExecutionRunThread,
  listChildExecutionSessions,
  listExecutionContainers,
  listExecutionSessions,
} from '@/api/execution-console'
import { rebuildExternalChatSession } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type {
  AgentFile,
  BackgroundTaskFolder,
  ChildRunLoadState,
  ExternalChannelFolder,
  RunListItem,
  WorkspaceAgentModelSelection,
  WorkspaceSessionFolder,
} from '@/types/workspace'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import type { ExecutionContainerSummary } from '@/types/generated/ExecutionContainerSummary'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ThreadSelection } from '@/components/chat/threadItems'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'

const toast = useToast()
const { t } = useI18n()
const { confirm } = useConfirm()
const route = useRoute()
const router = useRouter()
const chatSessionStore = useChatSessionStore()
const backgroundAgentStore = useBackgroundAgentStore()
const { isDark, toggleDark } = useTheme()

const showSettings = ref(false)

const availableAgents = ref<AgentFile[]>([])
const agentModelById = ref<Map<string, string>>(new Map())
const sidebarMode = ref<'sessions' | 'agents'>('sessions')
const selectedAgentId = ref<string | null>(null)
const selectedSessionId = ref<string | null>(null)
const activeContainerId = ref<string | null>(null)
const activeRunId = ref<string | null>(null)
const activeBackgroundTaskId = ref<string | null>(null)
const activeExecutionThread = ref<ExecutionThread | null>(null)
const runThreadByRunId = ref<Record<string, ExecutionThread>>({})
const executionContainers = ref<ExecutionContainerSummary[]>([])
const expandedWorkspaceContainerIds = ref<Set<string>>(new Set())
const workspaceRunsByContainerId = ref<Record<string, ExecutionSessionSummary[]>>({})
const expandedBackgroundTaskIds = ref<Set<string>>(new Set())
const backgroundRunsByTaskId = ref<Record<string, ExecutionSessionSummary[]>>({})
const expandedExternalContainerIds = ref<Set<string>>(new Set())
const externalRunsByContainerId = ref<Record<string, ExecutionSessionSummary[]>>({})
const childRunsByParentRunId = ref<Record<string, ExecutionSessionSummary[]>>({})
const childRunStateByParentRunId = ref<Record<string, ChildRunLoadState>>({})
const childRunErrorByParentRunId = ref<Record<string, string | null>>({})
const toolPanel = useToolPanel()
const isSending = computed(() => chatSessionStore.isSending)

const renameDialogOpen = ref(false)
const renameSessionId = ref('')
const renameSessionValue = ref('')

const convertDialogOpen = ref(false)
const convertSessionId = ref('')
const convertSessionName = ref('')

const createAgentDialogOpen = ref(false)
const DEFAULT_SIDEBAR_RATIO = 0.2
const MIN_SIDEBAR_RATIO = 0.16
const MAX_SIDEBAR_RATIO = 0.34
const SIDEBAR_RATIO_STORAGE_KEY = 'workspace-sidebar-ratio'
const workspaceContentRef = ref<HTMLElement | null>(null)
const sidebarRatio = ref(DEFAULT_SIDEBAR_RATIO)
const isSidebarResizing = ref(false)
const sidebarResizeStartX = ref(0)
const sidebarResizeStartRatio = ref(DEFAULT_SIDEBAR_RATIO)
let routeResolutionVersion = 0
let toolPanelRunNavigationVersion = 0

interface ToolPanelRunNavigationNode {
  key: 'root' | 'parent' | 'current'
  runId: string
  containerId: string
  label: string
  badge: string
  clickable: boolean
}

const toolPanelRunNavigation = ref<ToolPanelRunNavigationNode[]>([])

const sidebarStyle = computed(() => ({
  width: `${(sidebarRatio.value * 100).toFixed(2)}%`,
}))

function clampSidebarRatio(value: number): number {
  return Math.min(MAX_SIDEBAR_RATIO, Math.max(MIN_SIDEBAR_RATIO, value))
}

function persistSidebarRatio(ratio: number) {
  window.localStorage.setItem(SIDEBAR_RATIO_STORAGE_KEY, String(ratio))
}

function workspaceContentWidth(): number {
  return workspaceContentRef.value?.clientWidth || window.innerWidth || 1
}

function applySidebarRatio(ratio: number) {
  const nextRatio = clampSidebarRatio(ratio)
  sidebarRatio.value = nextRatio
  persistSidebarRatio(nextRatio)
}

function handleSidebarResizeMove(event: MouseEvent) {
  if (!isSidebarResizing.value) return
  const containerWidth = workspaceContentWidth()
  if (containerWidth <= 0) return
  const delta = event.clientX - sidebarResizeStartX.value
  applySidebarRatio(sidebarResizeStartRatio.value + delta / containerWidth)
}

function stopSidebarResize() {
  if (!isSidebarResizing.value) return
  isSidebarResizing.value = false
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}

function startSidebarResize(event: MouseEvent) {
  isSidebarResizing.value = true
  sidebarResizeStartX.value = event.clientX
  sidebarResizeStartRatio.value = sidebarRatio.value
  document.body.style.cursor = 'col-resize'
  document.body.style.userSelect = 'none'
  event.preventDefault()
}

function normalizeSessionStatus(status: string, isSelected: boolean): WorkspaceSessionFolder['status'] {
  if (isSelected && isSending.value) {
    return 'running'
  }

  switch (status) {
    case 'running':
      return 'running'
    case 'failed':
    case 'interrupted':
    case 'error':
      return 'failed'
    case 'pending':
      return 'pending'
    default:
      return 'completed'
  }
}

function agentNameForId(agentId: string | null | undefined): string | undefined {
  if (!agentId) return undefined
  return availableAgents.value.find((agent) => agent.id === agentId)?.name ?? agentId
}

function toRunListItem(summary: ExecutionSessionSummary, path = new Set<string>()): RunListItem {
  const runId = summary.run_id ?? null
  const nextPath = new Set(path)
  if (runId) {
    nextPath.add(runId)
  }
  const childRuns =
    runId && !path.has(runId)
      ? (childRunsByParentRunId.value[runId] ?? []).map((child) => toRunListItem(child, nextPath))
      : []

  return {
    id: summary.id,
    title: summary.title,
    status: summary.status,
    updatedAt: summary.updated_at,
    runId,
    agentName: agentNameForId(summary.agent_id),
    childRunsState: runId ? (childRunStateByParentRunId.value[runId] ?? 'idle') : 'loaded',
    childRunsError: runId ? (childRunErrorByParentRunId.value[runId] ?? null) : null,
    childRuns,
  }
}

const workspaceContainers = computed(() =>
  executionContainers.value.filter((container) => container.kind === 'workspace'),
)

const backgroundTaskContainers = computed(() =>
  executionContainers.value.filter((container) => container.kind === 'background_task'),
)

const externalChannelContainers = computed(() =>
  executionContainers.value.filter((container) => container.kind === 'external_channel'),
)

const workspaceFolders = computed<WorkspaceSessionFolder[]>(() =>
  workspaceContainers.value
    .map((container) => ({
      containerId: container.id,
      sessionId: container.latest_session_id ?? container.id,
      name: container.title,
      subtitle: container.subtitle ?? null,
      status: normalizeSessionStatus(
        container.status ?? 'pending',
        activeContainerId.value === container.id && !activeRunId.value,
      ),
      updatedAt: container.updated_at,
      expanded: expandedWorkspaceContainerIds.value.has(container.id),
      agentId: container.agent_id ?? undefined,
      agentName: agentNameForId(container.agent_id),
      sourceChannel: container.source_channel ?? null,
      runs: (workspaceRunsByContainerId.value[container.id] ?? []).map((summary) => toRunListItem(summary)),
    }))
    .sort((left, right) => right.updatedAt - left.updatedAt || left.containerId.localeCompare(right.containerId)),
)

const backgroundFolders = computed<BackgroundTaskFolder[]>(() =>
  backgroundTaskContainers.value.map((container) => ({
    taskId: container.id,
    chatSessionId: container.latest_session_id ?? null,
    name: container.title,
    subtitle: container.subtitle ?? null,
    status: container.status ?? 'idle',
    updatedAt: container.updated_at,
    expanded: expandedBackgroundTaskIds.value.has(container.id),
    runs: (backgroundRunsByTaskId.value[container.id] ?? []).map((summary) => toRunListItem(summary)),
  })),
)

const externalFolders = computed<ExternalChannelFolder[]>(() =>
  externalChannelContainers.value.map((container) => ({
    containerId: container.id,
    latestSessionId: container.latest_session_id ?? null,
    name: container.title,
    subtitle: container.subtitle ?? null,
    status: container.status ?? null,
    updatedAt: container.updated_at,
    expanded: expandedExternalContainerIds.value.has(container.id),
    sourceChannel: container.source_channel ?? null,
    runs: (externalRunsByContainerId.value[container.id] ?? []).map((summary) => toRunListItem(summary)),
  })),
)

const routeContainerId = computed(() => {
  if (route.name !== 'workspace-container' && route.name !== 'workspace-container-run') {
    return ''
  }
  return String(route.params.containerId ?? '').trim()
})

const routeContainerRunId = computed(() => {
  if (route.name !== 'workspace-container-run') {
    return ''
  }
  return String(route.params.runId ?? '').trim()
})

const activeContainer = computed(() => {
  const containerId = activeContainerId.value || routeContainerId.value
  return containerId ? findContainerById(containerId) : null
})

const showContainerNotFoundState = computed(
  () =>
    sidebarMode.value === 'sessions' &&
    !!routeContainerId.value &&
    !routeContainerRunId.value &&
    !activeContainer.value,
)

const showContainerEmptyState = computed(
  () =>
    sidebarMode.value === 'sessions' &&
    !!routeContainerId.value &&
    !routeContainerRunId.value &&
    !!activeContainer.value &&
    !selectedSessionId.value,
)

const containerEmptyStateTitle = computed(() => activeContainer.value?.title ?? 'Container')
const containerEmptyStateDescription = computed(() => {
  switch (activeContainer.value?.kind) {
    case 'background_task':
      return 'No runs have been created for this background agent yet.'
    case 'external_channel':
      return 'No runs have been created for this external channel yet.'
    default:
      return 'No runs have been created for this container yet.'
  }
})
const containerNotFoundTitle = computed(() => t('workspace.container.notFoundTitle'))
const containerNotFoundDescription = computed(() => t('workspace.container.notFoundDescription'))

const chatPanelSelectedRunId = computed(() => activeRunId.value ?? (routeContainerRunId.value || null))
const chatPanelAutoSelectRecent = computed(() => !routeContainerId.value && !routeContainerRunId.value)
const showRunOverviewPanel = computed(
  () =>
    sidebarMode.value === 'sessions' &&
    !!chatPanelSelectedRunId.value &&
    !toolPanel.visible.value &&
    !!activeExecutionThread.value,
)
const activeRunChildSessions = computed(() => {
  const runId = activeExecutionThread.value?.focus.run_id ?? null
  if (!runId) return []
  return childRunsByParentRunId.value[runId] ?? []
})

async function clearWorkspaceSelection(containerId: string | null = null) {
  activeContainerId.value = containerId
  activeRunId.value = null
  activeExecutionThread.value = null
  toolPanelRunNavigation.value = []
  activeBackgroundTaskId.value =
    containerId && findContainerById(containerId)?.kind === 'background_task' ? containerId : null
  selectedSessionId.value = null
  await chatSessionStore.selectSession(null)
  toolPanel.clearHistory()
}

function isExternallyManagedSession(sessionId: string): boolean {
  const container = executionContainers.value.find(
    (entry) => entry.latest_session_id === sessionId || entry.id === sessionId,
  )
  return !!container?.source_channel && container.source_channel !== 'workspace'
}

function canonicalContainerRoute(containerId: string) {
  return {
    name: 'workspace-container' as const,
    params: { containerId },
  }
}

function canonicalContainerRunRoute(containerId: string, runId: string) {
  return {
    name: 'workspace-container-run' as const,
    params: { containerId, runId },
  }
}

function findContainerById(containerId: string): ExecutionContainerSummary | null {
  return executionContainers.value.find((container) => container.id === containerId) ?? null
}

async function loadAgents() {
  try {
    const agents = await listAgents()
    availableAgents.value = agents.map((agent) => ({
      id: agent.id,
      name: agent.name,
      path: `agents/${agent.id}`,
    }))
    agentModelById.value = new Map(agents.map((agent) => [agent.id, agent.agent.model ?? 'gpt-5']))

    if (!selectedAgentId.value && agents.length > 0) {
      selectedAgentId.value = agents[0]?.id ?? null
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadAgentsFailed')
    toast.error(message)
  }
}

async function onNewSession() {
  const referenceSession = chatSessionStore.currentSession ?? chatSessionStore.summaries[0] ?? null
  const fallbackAgentId = referenceSession?.agent_id ?? availableAgents.value[0]?.id ?? null
  if (!fallbackAgentId) {
    toast.error(t('chat.selectAgentToStart'))
    return
  }
  const fallbackModel = referenceSession?.model ?? agentModelById.value.get(fallbackAgentId) ?? 'gpt-5'

  const session = await chatSessionStore.createSession(fallbackAgentId, fallbackModel)
  if (!session) {
    toast.error(chatSessionStore.error || t('chat.createSessionFailed'))
    return
  }

  selectedSessionId.value = session.id
  activeContainerId.value = session.id
  activeRunId.value = null
  activeBackgroundTaskId.value = null
  await chatSessionStore.selectSession(session.id)
  await refreshNavigationProjection()
  await ensureWorkspaceRunsLoaded(session.id)
  const next = new Set(expandedWorkspaceContainerIds.value)
  next.add(session.id)
  expandedWorkspaceContainerIds.value = next
  await router.push(canonicalContainerRoute(session.id))
}

function onSwitchToSessions() {
  sidebarMode.value = 'sessions'
  if (routeContainerId.value || routeContainerRunId.value) {
    return
  }
  void router.push({ name: 'workspace' })
}

function onSwitchToAgents() {
  sidebarMode.value = 'agents'
  void router.push({ name: 'workspace' })
  if (!selectedAgentId.value && availableAgents.value.length > 0) {
    selectedAgentId.value = availableAgents.value[0]?.id ?? null
  }
}

function onSelectAgent(agentId: string) {
  selectedAgentId.value = agentId
  sidebarMode.value = 'agents'
  void router.push({ name: 'workspace' })
}

function onShowPanel(resultJson: string) {
  toolPanel.handleShowPanelResult(resultJson)
}

function onToolResult(step: StreamStep) {
  toolPanel.handleToolResult(step)
}

function onThreadSelection(selection: ThreadSelection) {
  toolPanel.handleThreadSelection(selection)
}

function cacheExecutionThread(thread: ExecutionThread) {
  const runId = thread.focus.run_id ?? null
  if (!runId) return
  runThreadByRunId.value = {
    ...runThreadByRunId.value,
    [runId]: thread,
  }
}

async function syncToolPanelRunNavigation(thread: ExecutionThread | null) {
  const version = ++toolPanelRunNavigationVersion
  const focus = thread?.focus ?? null

  if (!focus?.run_id || focus.kind !== 'subagent_run') {
    toolPanelRunNavigation.value = []
    return
  }

  const runLabels = new Map<string, string>()
  runLabels.set(focus.run_id, focus.title || focus.run_id)

  const runIdsToLoad = new Set<string>()
  if (focus.root_run_id && focus.root_run_id !== focus.run_id) {
    runIdsToLoad.add(focus.root_run_id)
  }
  if (
    focus.parent_run_id &&
    focus.parent_run_id !== focus.run_id &&
    focus.parent_run_id !== focus.root_run_id
  ) {
    runIdsToLoad.add(focus.parent_run_id)
  }

  await Promise.all(
    [...runIdsToLoad].map(async (runId) => {
      try {
        const resolvedThread = await ensureRunThreadLoaded(runId)
        runLabels.set(runId, resolvedThread.focus.title || runId)
      } catch {
        runLabels.set(runId, runId)
      }
    }),
  )

  if (version !== toolPanelRunNavigationVersion) return

  const nodes: ToolPanelRunNavigationNode[] = []
  if (focus.root_run_id && focus.root_run_id !== focus.run_id) {
    nodes.push({
      key: 'root',
      runId: focus.root_run_id,
      containerId: focus.container_id,
      label: runLabels.get(focus.root_run_id) ?? 'Root run',
      badge: 'Root',
      clickable: true,
    })
  }
  if (
    focus.parent_run_id &&
    focus.parent_run_id !== focus.run_id &&
    focus.parent_run_id !== focus.root_run_id
  ) {
    nodes.push({
      key: 'parent',
      runId: focus.parent_run_id,
      containerId: focus.container_id,
      label: runLabels.get(focus.parent_run_id) ?? 'Parent run',
      badge: 'Parent',
      clickable: true,
    })
  }
  nodes.push({
    key: 'current',
    runId: focus.run_id,
    containerId: focus.container_id,
    label: runLabels.get(focus.run_id) ?? focus.run_id,
    badge: 'Child',
    clickable: false,
  })

  toolPanelRunNavigation.value = nodes
}

function onThreadLoaded(thread: ExecutionThread | null) {
  activeExecutionThread.value = thread
  if (thread) {
    cacheExecutionThread(thread)
  }
  void syncToolPanelRunNavigation(thread)
  const runId = thread?.focus.run_id ?? null
  const containerId = thread?.focus.container_id ?? null
  if (runId) {
    void ensureChildRunsLoaded(runId)
  }
  if (!runId || !containerId) return
  if (routeContainerRunId.value === runId && routeContainerId.value === containerId) return

  void router.replace(canonicalContainerRunRoute(containerId, runId))
}

async function loadExecutionContainersProjection() {
  executionContainers.value = await listExecutionContainers()
}

async function refreshNavigationProjection() {
  await loadExecutionContainersProjection()
}

async function ensureRunThreadLoaded(
  runId: string,
  forceRefresh = false,
): Promise<ExecutionThread> {
  if (!forceRefresh && runThreadByRunId.value[runId]) {
    return runThreadByRunId.value[runId]
  }

  const thread = await getExecutionRunThread(runId)
  cacheExecutionThread(thread)
  return thread
}

async function ensureChildRunsLoaded(
  parentRunId: string,
  forceRefresh = false,
): Promise<ExecutionSessionSummary[]> {
  if (!forceRefresh && childRunStateByParentRunId.value[parentRunId] === 'loaded') {
    return childRunsByParentRunId.value[parentRunId] ?? []
  }

  childRunStateByParentRunId.value = {
    ...childRunStateByParentRunId.value,
    [parentRunId]: 'loading',
  }
  childRunErrorByParentRunId.value = {
    ...childRunErrorByParentRunId.value,
    [parentRunId]: null,
  }

  try {
    const runs = await listChildExecutionSessions({
      parent_run_id: parentRunId,
    })

    childRunsByParentRunId.value = {
      ...childRunsByParentRunId.value,
      [parentRunId]: runs,
    }
    childRunStateByParentRunId.value = {
      ...childRunStateByParentRunId.value,
      [parentRunId]: 'loaded',
    }
    childRunErrorByParentRunId.value = {
      ...childRunErrorByParentRunId.value,
      [parentRunId]: null,
    }
    return runs
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.noSessions')
    childRunStateByParentRunId.value = {
      ...childRunStateByParentRunId.value,
      [parentRunId]: 'error',
    }
    childRunErrorByParentRunId.value = {
      ...childRunErrorByParentRunId.value,
      [parentRunId]: message,
    }
    throw error
  }
}

async function ensureRunAncestorChildrenLoaded(focus: ExecutionSessionSummary): Promise<void> {
  const ancestors: string[] = []
  let currentParentRunId = focus.parent_run_id ?? null

  while (currentParentRunId) {
    ancestors.push(currentParentRunId)
    const parentThread = await ensureRunThreadLoaded(currentParentRunId)
    currentParentRunId = parentThread.focus.parent_run_id ?? null
  }

  await Promise.all(ancestors.map((runId) => ensureChildRunsLoaded(runId)))
}

async function ensureWorkspaceRunsLoaded(
  containerId: string,
  forceRefresh = false,
): Promise<ExecutionSessionSummary[]> {
  if (!forceRefresh && workspaceRunsByContainerId.value[containerId]) {
    return workspaceRunsByContainerId.value[containerId]
  }

  const runs = await listExecutionSessions({
    container: {
      kind: 'workspace',
      id: containerId,
    },
  })

  workspaceRunsByContainerId.value = {
    ...workspaceRunsByContainerId.value,
    [containerId]: runs,
  }
  return runs
}

async function ensureBackgroundRunsLoaded(
  taskId: string,
  forceRefresh = false,
): Promise<ExecutionSessionSummary[]> {
  if (!forceRefresh && backgroundRunsByTaskId.value[taskId]) {
    return backgroundRunsByTaskId.value[taskId]
  }

  const runs = await listExecutionSessions({
    container: {
      kind: 'background_task',
      id: taskId,
    },
  })

  backgroundRunsByTaskId.value = {
    ...backgroundRunsByTaskId.value,
    [taskId]: runs,
  }
  return runs
}

async function ensureExternalRunsLoaded(
  containerId: string,
  forceRefresh = false,
): Promise<ExecutionSessionSummary[]> {
  if (!forceRefresh && externalRunsByContainerId.value[containerId]) {
    return externalRunsByContainerId.value[containerId]
  }

  const runs = await listExecutionSessions({
    container: {
      kind: 'external_channel',
      id: containerId,
    },
  })

  externalRunsByContainerId.value = {
    ...externalRunsByContainerId.value,
    [containerId]: runs,
  }
  return runs
}

async function onToggleRunChildren(_containerId: string, runId: string) {
  try {
    await ensureChildRunsLoaded(runId)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.noSessions')
    toast.error(message)
  }
}

async function onToggleWorkspaceFolder(containerId: string) {
  const next = new Set(expandedWorkspaceContainerIds.value)
  if (next.has(containerId)) {
    next.delete(containerId)
    expandedWorkspaceContainerIds.value = next
    return
  }

  next.add(containerId)
  expandedWorkspaceContainerIds.value = next
  try {
    await ensureWorkspaceRunsLoaded(containerId)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.noSessions')
    toast.error(message)
  }
}

async function onToggleBackgroundTask(taskId: string) {
  const next = new Set(expandedBackgroundTaskIds.value)
  if (next.has(taskId)) {
    next.delete(taskId)
    expandedBackgroundTaskIds.value = next
    return
  }

  next.add(taskId)
  expandedBackgroundTaskIds.value = next
  try {
    await ensureBackgroundRunsLoaded(taskId)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('backgroundAgent.runTraceDescription')
    toast.error(message)
  }
}

async function onToggleExternalChannel(containerId: string) {
  const next = new Set(expandedExternalContainerIds.value)
  if (next.has(containerId)) {
    next.delete(containerId)
    expandedExternalContainerIds.value = next
    return
  }

  next.add(containerId)
  expandedExternalContainerIds.value = next
  try {
    await ensureExternalRunsLoaded(containerId)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.noSessions')
    toast.error(message)
  }
}

async function onSelectContainer(kind: 'workspace' | 'background_task' | 'external_channel', containerId: string) {
  sidebarMode.value = 'sessions'
  const container = findContainerById(containerId)
  const latestRunId = container?.latest_run_id ?? null
  const sessionId = container?.latest_session_id ?? null

  activeContainerId.value = containerId
  activeRunId.value = null
  activeBackgroundTaskId.value = kind === 'background_task' ? containerId : null
  selectedSessionId.value = sessionId
  await chatSessionStore.selectSession(sessionId)

  if (latestRunId) {
    await router.push(canonicalContainerRunRoute(containerId, latestRunId))
    return
  }

  await router.push(canonicalContainerRoute(containerId))
}

async function onSelectRun(containerId: string, runId: string) {
  sidebarMode.value = 'sessions'
  await router.push(canonicalContainerRunRoute(containerId, runId))
}

async function expandContainerForFocus(focus: ExecutionSessionSummary, forceRefreshRuns = false) {
  if (focus.kind === 'background_run' && focus.task_id) {
    activeBackgroundTaskId.value = focus.task_id
    const next = new Set(expandedBackgroundTaskIds.value)
    next.add(focus.task_id)
    expandedBackgroundTaskIds.value = next
    await ensureBackgroundRunsLoaded(focus.task_id, forceRefreshRuns)
    return
  }

  if (focus.kind === 'workspace_run') {
    activeBackgroundTaskId.value = null
    const next = new Set(expandedWorkspaceContainerIds.value)
    next.add(focus.container_id)
    expandedWorkspaceContainerIds.value = next
    await ensureWorkspaceRunsLoaded(focus.container_id, forceRefreshRuns)
    return
  }

  if (focus.kind === 'external_run') {
    activeBackgroundTaskId.value = null
    const next = new Set(expandedExternalContainerIds.value)
    next.add(focus.container_id)
    expandedExternalContainerIds.value = next
    await ensureExternalRunsLoaded(focus.container_id, forceRefreshRuns)
    return
  }

  const container = findContainerById(focus.container_id)
  if (focus.kind === 'subagent_run' && container) {
    activeBackgroundTaskId.value = container.kind === 'background_task' ? container.id : null

    if (container.kind === 'workspace') {
      const next = new Set(expandedWorkspaceContainerIds.value)
      next.add(container.id)
      expandedWorkspaceContainerIds.value = next
      await ensureWorkspaceRunsLoaded(container.id, forceRefreshRuns)
      return
    }

    if (container.kind === 'background_task') {
      const next = new Set(expandedBackgroundTaskIds.value)
      next.add(container.id)
      expandedBackgroundTaskIds.value = next
      await ensureBackgroundRunsLoaded(container.id, forceRefreshRuns)
      return
    }

    const next = new Set(expandedExternalContainerIds.value)
    next.add(container.id)
    expandedExternalContainerIds.value = next
    await ensureExternalRunsLoaded(container.id, forceRefreshRuns)
    return
  }

  activeBackgroundTaskId.value = null
}

async function resolveRunRoute(runId: string, version: number, expectedContainerId: string | null = null) {
  const thread = await ensureRunThreadLoaded(runId, true)

  if (version !== routeResolutionVersion) return

  activeExecutionThread.value = thread
  void syncToolPanelRunNavigation(thread)
  const resolvedContainerId = thread.focus.container_id
  activeContainerId.value = resolvedContainerId
  activeRunId.value = thread.focus.run_id ?? runId
  selectedSessionId.value = thread.focus.session_id ?? null
  await chatSessionStore.selectSession(thread.focus.session_id ?? null)
  await expandContainerForFocus(thread.focus, true)
  await ensureRunAncestorChildrenLoaded(thread.focus)
  if (thread.focus.run_id) {
    await ensureChildRunsLoaded(thread.focus.run_id, true)
  }

  if (
    routeContainerRunId.value !== runId ||
    routeContainerId.value !== resolvedContainerId ||
    (expectedContainerId && expectedContainerId !== resolvedContainerId)
  ) {
    await router.replace(canonicalContainerRunRoute(resolvedContainerId, runId))
  }
}

function onNavigateToolPanelRun(payload: { containerId: string; runId: string }) {
  void router.push(canonicalContainerRunRoute(payload.containerId, payload.runId))
}

async function resolveContainerRoute(containerId: string, version: number) {
  const container = findContainerById(containerId)
  if (!container) {
    await clearWorkspaceSelection(containerId)
    return
  }

  if (container.latest_run_id) {
    await router.replace(canonicalContainerRunRoute(container.id, container.latest_run_id))
    return
  }

  if (version !== routeResolutionVersion) return

  activeContainerId.value = container.id
  activeRunId.value = null
  activeBackgroundTaskId.value = container.kind === 'background_task' ? container.id : null
  selectedSessionId.value = container.latest_session_id ?? null

  if (container.kind === 'workspace') {
    const next = new Set(expandedWorkspaceContainerIds.value)
    next.add(container.id)
    expandedWorkspaceContainerIds.value = next
  } else if (container.kind === 'background_task') {
    const next = new Set(expandedBackgroundTaskIds.value)
    next.add(container.id)
    expandedBackgroundTaskIds.value = next
  } else {
    const next = new Set(expandedExternalContainerIds.value)
    next.add(container.id)
    expandedExternalContainerIds.value = next
  }

  await chatSessionStore.selectSession(container.latest_session_id ?? null)
}

function findContainerBySessionId(sessionId: string): ExecutionContainerSummary | null {
  return (
    executionContainers.value.find(
      (container) => container.id === sessionId || container.latest_session_id === sessionId,
    ) ?? null
  )
}

async function onDeleteSession(id: string, name: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
  const confirmed = await confirmDelete(name, 'session')
  if (!confirmed) return

  const success = await chatSessionStore.deleteSession(id)
  if (success) {
    await refreshNavigationProjection()
    toast.success(t('workspace.session.deleteSuccess'))
    if (selectedSessionId.value === id) {
      selectedSessionId.value = null
      activeContainerId.value = null
      activeRunId.value = null
      await chatSessionStore.selectSession(null)
      await router.push({ name: 'workspace' })
    }
  } else {
    toast.error(t('workspace.session.deleteFailed'))
  }
}

async function onArchiveSession(id: string, _name: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }

  const success = await chatSessionStore.archiveSession(id)
  if (success) {
    await refreshNavigationProjection()
    toast.success(t('workspace.session.archiveSuccess'))
    if (selectedSessionId.value === id) {
      selectedSessionId.value = null
      activeContainerId.value = null
      activeRunId.value = null
      await chatSessionStore.selectSession(null)
      await router.push({ name: 'workspace' })
    }
  } else {
    toast.error(t('workspace.session.archiveFailed'))
  }
}

function onRenameSession(id: string, currentName: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
  renameSessionId.value = id
  renameSessionValue.value = currentName
  renameDialogOpen.value = true
}

async function submitRename() {
  const trimmed = renameSessionValue.value.trim()
  if (!trimmed) return

  const result = await chatSessionStore.renameSession(renameSessionId.value, trimmed)
  if (result) {
    await refreshNavigationProjection()
    toast.success(t('workspace.session.renameSuccess'))
  }
  renameDialogOpen.value = false
}

function onConvertToBackgroundAgent(id: string, name: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
  convertSessionId.value = id
  convertSessionName.value = name
  convertDialogOpen.value = true
}

async function onConvertToWorkspaceSession(id: string, name: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }

  const confirmed = await confirm({
    title: t('workspace.session.convertToWorkspace'),
    description: t('workspace.session.convertToWorkspaceDescription', { name }),
    confirmText: t('workspace.session.convertToWorkspaceConfirm'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })
  if (!confirmed) return

  const success = await backgroundAgentStore.convertSessionToWorkspace(id)
  if (!success) {
    toast.error(backgroundAgentStore.error || t('workspace.session.convertToWorkspaceFailed'))
    return
  }

  toast.success(t('workspace.session.convertToWorkspaceSuccess'))
  await chatSessionStore.fetchSummaries()
  await refreshNavigationProjection()
  const container = findContainerBySessionId(id)
  if (container?.latest_run_id) {
    await router.push(canonicalContainerRunRoute(container.id, container.latest_run_id))
    return
  }
  await router.push(canonicalContainerRoute(container?.id ?? id))
}

async function onRebuildSession(id: string, name: string) {
  if (!isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.rebuildFailed'))
    return
  }

  const confirmed = await confirm({
    title: t('workspace.session.rebuild'),
    description: t('workspace.session.rebuildDescription', { name }),
    confirmText: t('workspace.session.rebuildConfirm'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })
  if (!confirmed) return

  try {
    const rebuilt = await rebuildExternalChatSession(id)
    await chatSessionStore.fetchSummaries()
    await refreshNavigationProjection()
    selectedSessionId.value = rebuilt.id
    activeContainerId.value = findContainerBySessionId(rebuilt.id)?.id ?? rebuilt.id
    activeRunId.value = null
    await chatSessionStore.selectSession(rebuilt.id)
    const rebuiltContainer = findContainerBySessionId(rebuilt.id)
    if (rebuiltContainer?.latest_run_id) {
      await router.push(canonicalContainerRunRoute(rebuiltContainer.id, rebuiltContainer.latest_run_id))
    } else if (rebuiltContainer) {
      await router.push(canonicalContainerRoute(rebuiltContainer.id))
    }
    toast.success(t('workspace.session.rebuildSuccess'))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.session.rebuildFailed')
    toast.error(message)
  }
}

function onCreateAgent() {
  createAgentDialogOpen.value = true
}

async function onDeleteAgent(id: string, name: string) {
  if (isProtectedDefaultAssistant(id)) {
    toast.error(t('workspace.agent.deleteDefaultBlocked'))
    return
  }

  const confirmed = await confirmDelete(name, 'agent')
  if (!confirmed) return

  try {
    await deleteAgentApi(id)
    availableAgents.value = availableAgents.value.filter((agent) => agent.id !== id)
    agentModelById.value.delete(id)

    if (selectedAgentId.value === id) {
      selectedAgentId.value = availableAgents.value[0]?.id ?? null
    }

    if (chatSessionStore.currentSession?.agent_id === id) {
      selectedSessionId.value = null
      await chatSessionStore.selectSession(null)
    }

    toast.success(t('workspace.agent.deleteSuccess'))
    await chatSessionStore.fetchSummaries()
    await refreshNavigationProjection()
  } catch (error) {
    toast.error(resolveAgentDeleteErrorMessage(error))
  }
}

function onAgentCreated(agent: WorkspaceAgentModelSelection) {
  availableAgents.value.push({ id: agent.id, name: agent.name, path: `agents/${agent.id}` })
  agentModelById.value.set(agent.id, agent.model)
  selectedAgentId.value = agent.id
  sidebarMode.value = 'agents'
}

function onAgentUpdated(agent: WorkspaceAgentModelSelection) {
  const target = availableAgents.value.find((item) => item.id === agent.id)
  if (target) {
    target.name = agent.name
  }
  agentModelById.value.set(agent.id, agent.model)
}

function isProtectedDefaultAssistant(agentId: string): boolean {
  const target = availableAgents.value.find((item) => item.id === agentId)
  const normalized = target?.name?.trim().toLowerCase()
  return normalized === 'default assistant' || normalized === 'default'
}

function resolveAgentDeleteErrorMessage(error: unknown): string {
  if (!(error instanceof Error)) {
    return t('workspace.agent.deleteFailed')
  }

  const message = error.message.trim()
  const normalized = message.toLowerCase()

  if (normalized.includes('cannot delete default assistant agent')) {
    return t('workspace.agent.deleteDefaultBlocked')
  }
  if (normalized.includes('active background tasks exist')) {
    return t('workspace.agent.deleteBlockedByTasks')
  }
  if (normalized.includes('external channel sessions exist')) {
    return t('workspace.agent.deleteBlockedByExternalSessions')
  }

  return message || t('workspace.agent.deleteFailed')
}

watch(
  () => chatSessionStore.currentSessionId,
  (newId) => {
    if (newId && !routeContainerRunId.value) {
      selectedSessionId.value = newId
      activeContainerId.value = activeContainerId.value ?? newId
    }
  },
)

watch([selectedSessionId, routeContainerRunId], () => {
  toolPanel.clearHistory()
})

watch(
  () => routeContainerRunId.value,
  (runId, previousRunId) => {
    if (runId !== previousRunId) {
      activeExecutionThread.value = null
      toolPanelRunNavigation.value = []
    }
  },
)

watch(
  [routeContainerId, routeContainerRunId, executionContainers],
  async ([containerId, containerRunId]) => {
    routeResolutionVersion += 1
    const version = routeResolutionVersion

    if (sidebarMode.value !== 'agents') {
      sidebarMode.value = 'sessions'
    }

    try {
      if (containerRunId) {
        activeContainerId.value = containerId || null
        activeRunId.value = containerRunId
        activeBackgroundTaskId.value =
          containerId && findContainerById(containerId)?.kind === 'background_task'
            ? containerId
            : null
        selectedSessionId.value = null
        await chatSessionStore.selectSession(null)
        await resolveRunRoute(containerRunId, version, containerId || null)
        return
      }

      if (containerId) {
        await resolveContainerRoute(containerId, version)
        return
      }

      activeBackgroundTaskId.value = null
      activeRunId.value = null
      selectedSessionId.value = chatSessionStore.currentSessionId
      activeContainerId.value = chatSessionStore.currentSessionId
    } catch (error) {
      await clearWorkspaceSelection(containerId || null)
      if (containerRunId && containerId) {
        const container = findContainerById(containerId)
        const fallbackRoute =
          container?.latest_run_id && container.latest_run_id !== containerRunId
            ? canonicalContainerRoute(containerId)
            : { name: 'workspace' as const }
        await router.replace(fallbackRoute)
      }
      const message = error instanceof Error ? error.message : t('workspace.noSessions')
      toast.error(message)
    }
  },
  { immediate: true },
)

watch(convertDialogOpen, (open, previous) => {
  if (previous && !open) {
    void refreshNavigationProjection()
  }
})

onMounted(() => {
  const savedSidebarRatio = Number(window.localStorage.getItem(SIDEBAR_RATIO_STORAGE_KEY))
  if (Number.isFinite(savedSidebarRatio) && savedSidebarRatio > 0) {
    sidebarRatio.value = clampSidebarRatio(savedSidebarRatio)
  }

  window.addEventListener('mousemove', handleSidebarResizeMove)
  window.addEventListener('mouseup', stopSidebarResize)
  void loadAgents()
  void backgroundAgentStore.fetchAgents()
  void chatSessionStore.fetchSummaries()
  void refreshNavigationProjection()
})

onUnmounted(() => {
  window.removeEventListener('mousemove', handleSidebarResizeMove)
  window.removeEventListener('mouseup', stopSidebarResize)
  stopSidebarResize()
})
</script>

<template>
  <div class="h-screen flex bg-background" data-testid="workspace-shell">
    <SettingsPanel v-if="showSettings" class="flex-1" @back="showSettings = false" />

    <div v-show="!showSettings" ref="workspaceContentRef" class="flex flex-1 min-w-0" data-testid="workspace-content">
      <div
        class="border-r border-border shrink-0 flex flex-col"
        :style="sidebarStyle"
        data-testid="workspace-sidebar"
      >
        <div class="h-10 shrink-0 flex items-center pr-2">
          <div class="w-[5rem] shrink-0" data-testid="workspace-traffic-safe-zone" />
          <div
            class="ml-2 inline-flex items-center gap-1.5 select-none pointer-events-none"
            data-testid="workspace-brand"
          >
            <img src="/restflow.svg" alt="RestFlow logo" class="h-5 w-5 shrink-0 opacity-95" />
            <span class="text-sm font-semibold tracking-tight text-foreground/90">RestFlow</span>
          </div>
        </div>

        <div class="border-b border-border px-2 pt-2 pb-2">
          <div class="grid grid-cols-2 gap-1">
            <Button
              variant="ghost"
              size="sm"
              class="h-7 justify-start gap-1.5 text-xs"
              :class="sidebarMode === 'sessions' ? 'bg-muted' : ''"
              @click="onSwitchToSessions"
            >
              <MessageSquare :size="13" />
              {{ t('workspace.tabs.sessions') }}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-7 justify-start gap-1.5 text-xs"
              :class="sidebarMode === 'agents' ? 'bg-muted' : ''"
              @click="onSwitchToAgents"
            >
              <Bot :size="13" />
              {{ t('workspace.tabs.agents') }}
            </Button>
          </div>
        </div>

        <SessionList
          v-if="sidebarMode === 'sessions'"
          :workspace-folders="workspaceFolders"
          :background-folders="backgroundFolders"
          :external-folders="externalFolders"
          :current-container-id="activeContainerId"
          :current-run-id="activeRunId"
          class="flex-1 min-h-0"
          @new-session="onNewSession"
          @select-container="onSelectContainer"
          @select-run="onSelectRun"
          @rename="onRenameSession"
          @archive="onArchiveSession"
          @delete="onDeleteSession"
          @convert-to-background-agent="onConvertToBackgroundAgent"
          @convert-to-workspace-session="onConvertToWorkspaceSession"
          @rebuild="onRebuildSession"
          @toggle-workspace-folder="onToggleWorkspaceFolder"
          @toggle-background-task="onToggleBackgroundTask"
          @toggle-external-channel="onToggleExternalChannel"
          @toggle-run-children="onToggleRunChildren"
        />

        <AgentList
          v-else
          :agents="availableAgents"
          :selected-agent-id="selectedAgentId"
          class="flex-1 min-h-0"
          @select="onSelectAgent"
          @create="onCreateAgent"
          @delete="onDeleteAgent"
        />

        <div class="shrink-0 border-t border-border p-2 flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            :aria-label="t('common.settings')"
            @click="showSettings = true"
          >
            <Settings :size="14" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            :aria-label="t('common.theme')"
            @click="toggleDark()"
          >
            <Sun v-if="isDark" :size="14" />
            <Moon v-else :size="14" />
          </Button>
        </div>
      </div>

      <div
        class="w-1 shrink-0 cursor-col-resize bg-transparent transition-colors hover:bg-primary/20"
        :class="isSidebarResizing ? 'bg-primary/20' : ''"
        data-testid="workspace-sidebar-resizer"
        @mousedown="startSidebarResize"
      />

      <div
        v-if="showContainerNotFoundState"
        class="flex flex-1 min-w-0 items-center justify-center px-8"
        data-testid="workspace-container-not-found-state"
      >
        <div class="max-w-[28rem] space-y-2 text-center">
          <p class="text-base font-semibold">{{ containerNotFoundTitle }}</p>
          <p class="text-sm text-muted-foreground">{{ containerNotFoundDescription }}</p>
        </div>
      </div>

      <div
        v-else-if="showContainerEmptyState"
        class="flex flex-1 min-w-0 items-center justify-center px-8"
        data-testid="workspace-container-empty-state"
      >
        <div class="max-w-[28rem] space-y-2 text-center">
          <p class="text-base font-semibold">{{ containerEmptyStateTitle }}</p>
          <p class="text-sm text-muted-foreground">{{ containerEmptyStateDescription }}</p>
        </div>
      </div>

      <ChatPanel
        v-else-if="sidebarMode === 'sessions'"
        :selected-run-id="chatPanelSelectedRunId"
        :background-task-id="activeBackgroundTaskId"
        :auto-select-recent="chatPanelAutoSelectRecent"
        class="flex-1 min-w-0"
        @show-panel="onShowPanel"
        @tool-result="onToolResult"
        @thread-selection="onThreadSelection"
        @thread-loaded="onThreadLoaded"
      />

      <AgentEditorPanel
        v-else
        :agent-id="selectedAgentId"
        @back-to-sessions="onSwitchToSessions"
        @updated="onAgentUpdated"
      />

      <ToolPanel
        v-if="
          sidebarMode === 'sessions' &&
          (
            (toolPanel.visible.value && toolPanel.activeEntry.value) ||
            showRunOverviewPanel
          )
        "
        :mode="toolPanel.visible.value && toolPanel.activeEntry.value ? 'detail' : 'overview'"
        :panel-type="toolPanel.state.value.panelType"
        :title="toolPanel.state.value.title"
        :tool-name="toolPanel.state.value.toolName"
        :data="toolPanel.state.value.data"
        :step="toolPanel.state.value.step"
        :can-navigate-prev="toolPanel.canNavigatePrev.value"
        :can-navigate-next="toolPanel.canNavigateNext.value"
        :run-navigation="toolPanelRunNavigation"
        :run-thread="activeExecutionThread"
        :run-child-sessions="activeRunChildSessions"
        @navigate="toolPanel.navigateHistory"
        @navigate-run="onNavigateToolPanelRun"
        @close="toolPanel.closePanel()"
      />
    </div>

    <Dialog v-model:open="renameDialogOpen">
      <DialogContent class="max-w-[24rem]">
        <DialogHeader>
          <DialogTitle>{{ t('workspace.session.rename') }}</DialogTitle>
        </DialogHeader>
        <Input v-model="renameSessionValue" @keydown.enter="submitRename" />
        <DialogFooter>
          <Button variant="outline" @click="renameDialogOpen = false">
            {{ t('common.cancel') }}
          </Button>
          <Button :disabled="!renameSessionValue.trim()" @click="submitRename">
            {{ t('common.confirm') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <ConvertToBackgroundAgentDialog
      v-model:open="convertDialogOpen"
      :session-id="convertSessionId"
      :session-name="convertSessionName"
    />

    <CreateAgentDialog v-model:open="createAgentDialogOpen" @created="onAgentCreated" />
  </div>
</template>
