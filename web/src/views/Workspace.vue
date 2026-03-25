<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Container/run sidebar
 * - Center: Unified execution thread panel
 * - Right: Tool panel / inspector
 */
import { ref, computed, onMounted, watch } from 'vue'
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
  getExecutionThread,
  listExecutionContainers,
  listExecutionSessions,
} from '@/api/execution-console'
import { rebuildExternalChatSession } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type {
  AgentFile,
  BackgroundTaskFolder,
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
const executionContainers = ref<ExecutionContainerSummary[]>([])
const expandedWorkspaceContainerIds = ref<Set<string>>(new Set())
const workspaceRunsByContainerId = ref<Record<string, ExecutionSessionSummary[]>>({})
const expandedBackgroundTaskIds = ref<Set<string>>(new Set())
const backgroundRunsByTaskId = ref<Record<string, ExecutionSessionSummary[]>>({})
const expandedExternalContainerIds = ref<Set<string>>(new Set())
const externalRunsByContainerId = ref<Record<string, ExecutionSessionSummary[]>>({})
const toolPanel = useToolPanel()
const isSending = computed(() => chatSessionStore.isSending)

const renameDialogOpen = ref(false)
const renameSessionId = ref('')
const renameSessionValue = ref('')

const convertDialogOpen = ref(false)
const convertSessionId = ref('')
const convertSessionName = ref('')

const createAgentDialogOpen = ref(false)
let routeResolutionVersion = 0

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

function toRunListItem(summary: ExecutionSessionSummary): RunListItem {
  return {
    id: summary.id,
    title: summary.title,
    status: summary.status,
    updatedAt: summary.updated_at,
    runId: summary.run_id,
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
      runs: (workspaceRunsByContainerId.value[container.id] ?? []).map(toRunListItem),
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
    runs: (backgroundRunsByTaskId.value[container.id] ?? []).map(toRunListItem),
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
    runs: (externalRunsByContainerId.value[container.id] ?? []).map(toRunListItem),
  })),
)

const routeSessionId = computed(() => {
  if (route.name !== 'workspace-session') {
    return ''
  }
  return String(route.params.sessionId ?? '').trim()
})

const routeTaskId = computed(() => {
  if (route.name !== 'workspace-run') {
    return ''
  }
  return String(route.params.taskId ?? '').trim()
})

const routeRunId = computed(() => {
  if (route.name !== 'workspace-run-id') {
    return ''
  }
  return String(route.params.runId ?? '').trim()
})

function isExternallyManagedSession(sessionId: string): boolean {
  const container = executionContainers.value.find(
    (entry) => entry.latest_session_id === sessionId || entry.id === sessionId,
  )
  return !!container?.source_channel && container.source_channel !== 'workspace'
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
  await router.push({ name: 'workspace-session', params: { sessionId: session.id } })
}

function onSwitchToSessions() {
  sidebarMode.value = 'sessions'
  if (routeRunId.value || routeSessionId.value || routeTaskId.value) {
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

function onThreadLoaded(thread: ExecutionThread | null) {
  const runId = thread?.focus.run_id ?? null
  if (!runId) return
  if (routeRunId.value === runId) return
  if (!routeSessionId.value && !routeTaskId.value) return

  void router.replace({
    name: 'workspace-run-id',
    params: { runId },
  })
}

async function loadExecutionContainersProjection() {
  executionContainers.value = await listExecutionContainers()
}

async function refreshNavigationProjection() {
  await loadExecutionContainersProjection()
}

async function ensureWorkspaceRunsLoaded(containerId: string): Promise<ExecutionSessionSummary[]> {
  if (workspaceRunsByContainerId.value[containerId]) {
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

async function ensureBackgroundRunsLoaded(taskId: string): Promise<ExecutionSessionSummary[]> {
  if (backgroundRunsByTaskId.value[taskId]) {
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

async function ensureExternalRunsLoaded(containerId: string): Promise<ExecutionSessionSummary[]> {
  if (externalRunsByContainerId.value[containerId]) {
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

  if (kind === 'workspace') {
    activeContainerId.value = containerId
    activeRunId.value = null
    activeBackgroundTaskId.value = null
    selectedSessionId.value = containerId
    await chatSessionStore.selectSession(containerId)
    await router.push({ name: 'workspace-session', params: { sessionId: containerId } })
    return
  }

  if (kind === 'background_task') {
    activeContainerId.value = containerId
    activeRunId.value = null
    activeBackgroundTaskId.value = containerId
    let task = backgroundAgentStore.agents.find((agent) => agent.id === containerId) ?? null
    if (!task) {
      await backgroundAgentStore.fetchAgents()
      task = backgroundAgentStore.agents.find((agent) => agent.id === containerId) ?? null
    }
    const sessionId = task?.chat_session_id ?? null
    selectedSessionId.value = sessionId
    await chatSessionStore.selectSession(sessionId)
    await router.push({ name: 'workspace-run', params: { taskId: containerId } })
    return
  }

  const runs = await ensureExternalRunsLoaded(containerId)
  const latestRunId = runs[0]?.run_id ?? null
  if (latestRunId) {
    await router.push({ name: 'workspace-run-id', params: { runId: latestRunId } })
    return
  }

  const container = executionContainers.value.find((entry) => entry.id === containerId) ?? null
  const sessionId = container?.latest_session_id ?? null
  activeContainerId.value = containerId
  activeRunId.value = null
  activeBackgroundTaskId.value = null
  selectedSessionId.value = sessionId
  await chatSessionStore.selectSession(sessionId)
  if (sessionId) {
    await router.push({ name: 'workspace-session', params: { sessionId } })
  }
}

async function onSelectRun(runId: string) {
  sidebarMode.value = 'sessions'
  await router.push({ name: 'workspace-run-id', params: { runId } })
}

async function expandContainerForFocus(focus: ExecutionSessionSummary) {
  if (focus.kind === 'background_run' && focus.task_id) {
    activeBackgroundTaskId.value = focus.task_id
    const next = new Set(expandedBackgroundTaskIds.value)
    next.add(focus.task_id)
    expandedBackgroundTaskIds.value = next
    await ensureBackgroundRunsLoaded(focus.task_id)
    return
  }

  if (focus.kind === 'workspace_run') {
    activeBackgroundTaskId.value = null
    const next = new Set(expandedWorkspaceContainerIds.value)
    next.add(focus.container_id)
    expandedWorkspaceContainerIds.value = next
    await ensureWorkspaceRunsLoaded(focus.container_id)
    return
  }

  if (focus.kind === 'external_run') {
    activeBackgroundTaskId.value = null
    const next = new Set(expandedExternalContainerIds.value)
    next.add(focus.container_id)
    expandedExternalContainerIds.value = next
    await ensureExternalRunsLoaded(focus.container_id)
    return
  }

  activeBackgroundTaskId.value = null
}

async function resolveRunRoute(runId: string, version: number) {
  const thread = await getExecutionThread({
    run_id: runId,
    session_id: null,
    task_id: null,
  })

  if (version !== routeResolutionVersion) return

  activeContainerId.value = thread.focus.container_id
  activeRunId.value = thread.focus.run_id ?? runId
  selectedSessionId.value = thread.focus.session_id ?? null
  await chatSessionStore.selectSession(thread.focus.session_id ?? null)
  await expandContainerForFocus(thread.focus)
}

function findContainerBySessionId(sessionId: string): ExecutionContainerSummary | null {
  return (
    executionContainers.value.find(
      (container) => container.id === sessionId || container.latest_session_id === sessionId,
    ) ?? null
  )
}

async function resolveSessionRoute(sessionId: string, version: number) {
  const container = findContainerBySessionId(sessionId)

  if (container?.kind === 'background_task') {
    await router.replace({ name: 'workspace-run', params: { taskId: container.id } })
    return
  }

  const kind = container?.kind ?? 'workspace'
  const containerId = container?.id ?? sessionId
  const runs =
    kind === 'external_channel'
      ? await ensureExternalRunsLoaded(containerId)
      : await ensureWorkspaceRunsLoaded(containerId)
  if (version !== routeResolutionVersion) return

  const latestRunId = runs[0]?.run_id ?? null
  if (latestRunId) {
    await router.replace({ name: 'workspace-run-id', params: { runId: latestRunId } })
    return
  }

  activeContainerId.value = containerId
  activeRunId.value = null
  activeBackgroundTaskId.value = null
  selectedSessionId.value = sessionId
  if (kind === 'external_channel') {
    const next = new Set(expandedExternalContainerIds.value)
    next.add(containerId)
    expandedExternalContainerIds.value = next
  } else {
    const next = new Set(expandedWorkspaceContainerIds.value)
    next.add(containerId)
    expandedWorkspaceContainerIds.value = next
  }
  await chatSessionStore.selectSession(sessionId)
}

async function resolveTaskRoute(taskId: string, version: number) {
  const runs = await ensureBackgroundRunsLoaded(taskId)
  if (version !== routeResolutionVersion) return

  const preferredRunId = typeof route.query.runId === 'string' && route.query.runId.trim().length > 0
    ? route.query.runId.trim()
    : null
  const resolvedRunId =
    (preferredRunId && runs.find((entry) => entry.run_id === preferredRunId)?.run_id) ||
    runs[0]?.run_id ||
    null

  if (resolvedRunId) {
    await router.replace({ name: 'workspace-run-id', params: { runId: resolvedRunId } })
    return
  }

  activeContainerId.value = taskId
  activeRunId.value = null
  activeBackgroundTaskId.value = taskId
  const next = new Set(expandedBackgroundTaskIds.value)
  next.add(taskId)
  expandedBackgroundTaskIds.value = next

  let task = backgroundAgentStore.agents.find((agent) => agent.id === taskId) ?? null
  if (!task) {
    await backgroundAgentStore.fetchAgents()
    task = backgroundAgentStore.agents.find((agent) => agent.id === taskId) ?? null
  }
  selectedSessionId.value = task?.chat_session_id ?? null
  await chatSessionStore.selectSession(task?.chat_session_id ?? null)
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
  await router.push({ name: 'workspace-session', params: { sessionId: id } })
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
    if (newId && !routeRunId.value && !routeTaskId.value) {
      selectedSessionId.value = newId
      activeContainerId.value = activeContainerId.value ?? newId
    }
  },
)

watch([selectedSessionId, routeRunId], () => {
  toolPanel.clearHistory()
})

watch(
  [routeRunId, routeSessionId, routeTaskId, executionContainers],
  async ([runId, sessionId, taskId]) => {
    routeResolutionVersion += 1
    const version = routeResolutionVersion

    if (sidebarMode.value !== 'agents') {
      sidebarMode.value = 'sessions'
    }

    try {
      if (runId) {
        await resolveRunRoute(runId, version)
        return
      }

      if (taskId) {
        await resolveTaskRoute(taskId, version)
        return
      }

      if (sessionId) {
        await resolveSessionRoute(sessionId, version)
        return
      }

      activeBackgroundTaskId.value = null
      activeRunId.value = null
      selectedSessionId.value = chatSessionStore.currentSessionId
      activeContainerId.value = chatSessionStore.currentSessionId
    } catch (error) {
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
  void loadAgents()
  void backgroundAgentStore.fetchAgents()
  void chatSessionStore.fetchSummaries()
  void refreshNavigationProjection()
})
</script>

<template>
  <div class="h-screen flex bg-background" data-testid="workspace-shell">
    <SettingsPanel v-if="showSettings" class="flex-1" @back="showSettings = false" />

    <div v-show="!showSettings" class="flex flex-1 min-w-0">
      <div class="w-56 border-r border-border shrink-0 flex flex-col">
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

      <ChatPanel
        v-if="sidebarMode === 'sessions'"
        :selected-run-id="activeRunId"
        :background-task-id="activeBackgroundTaskId"
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
          toolPanel.visible.value &&
          toolPanel.activeEntry.value
        "
        :panel-type="toolPanel.state.value.panelType"
        :title="toolPanel.state.value.title"
        :tool-name="toolPanel.state.value.toolName"
        :data="toolPanel.state.value.data"
        :step="toolPanel.state.value.step"
        :can-navigate-prev="toolPanel.canNavigatePrev.value"
        :can-navigate-next="toolPanel.canNavigateNext.value"
        @navigate="toolPanel.navigateHistory"
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
