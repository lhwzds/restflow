<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Session/Agent sidebar
 * - Center: Chat panel or agent editor
 * - Right: Tool panel / inspector (session mode only)
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
import WorkspaceRunPanel from '@/components/workspace/WorkspaceRunPanel.vue'
import ConvertToBackgroundAgentDialog from '@/components/workspace/ConvertToBackgroundAgentDialog.vue'
import CreateAgentDialog from '@/components/workspace/CreateAgentDialog.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import { useTheme } from '@/composables/useTheme'
import { confirmDelete, useConfirm } from '@/composables/useConfirm'
import { deleteAgent as deleteAgentApi, listAgents } from '@/api/agents'
import { listExecutionContainers, listExecutionSessions } from '@/api/execution-console'
import { rebuildExternalChatSession } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type {
  AgentFile,
  BackgroundTaskFolder,
  ExternalChannelFolder,
  SessionItem,
  WorkspaceAgentModelSelection,
} from '@/types/workspace'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import type { ExecutionContainerSummary } from '@/types/generated/ExecutionContainerSummary'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ThreadSelection } from '@/components/chat/threadItems'

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

const selectedItemId = ref<string | null>(null)
const isSending = computed(() => chatSessionStore.isSending)
const executionContainers = ref<ExecutionContainerSummary[]>([])
const workspaceSessionsById = ref<Record<string, ExecutionSessionSummary>>({})
const expandedBackgroundTaskIds = ref<Set<string>>(new Set())
const backgroundRunsByTaskId = ref<Record<string, ExecutionSessionSummary[]>>({})
const expandedExternalContainerIds = ref<Set<string>>(new Set())
const externalSessionsByContainerId = ref<Record<string, ExecutionSessionSummary[]>>({})

const toolPanel = useToolPanel()

const renameDialogOpen = ref(false)
const renameSessionId = ref('')
const renameSessionValue = ref('')

const convertDialogOpen = ref(false)
const convertSessionId = ref('')
const convertSessionName = ref('')

const createAgentDialogOpen = ref(false)

function normalizeSessionStatus(status: string, isSelected: boolean): SessionItem['status'] {
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

function toSessionItem(summary: ExecutionSessionSummary): SessionItem {
  return {
    id: summary.session_id ?? summary.id,
    name: summary.title,
    subtitle: summary.subtitle ?? null,
    status: normalizeSessionStatus(summary.status, selectedItemId.value === (summary.session_id ?? summary.id)),
    updatedAt: summary.updated_at,
    agentId: summary.agent_id ?? undefined,
    agentName: agentNameForId(summary.agent_id),
    containerId: summary.container_id,
    sourceChannel: summary.source_channel ?? null,
  }
}

const workspaceContainer = computed(
  () => executionContainers.value.find((container) => container.kind === 'workspace') ?? null,
)

const backgroundTaskContainers = computed(() =>
  executionContainers.value.filter((container) => container.kind === 'background_task'),
)

const externalChannelContainers = computed(() =>
  executionContainers.value.filter((container) => container.kind === 'external_channel'),
)

const workspaceSessions = computed<SessionItem[]>(() =>
  Object.values(workspaceSessionsById.value)
    .sort((left, right) => right.updated_at - left.updated_at || left.id.localeCompare(right.id))
    .map((session) => toSessionItem(session)),
)

const currentBackgroundTaskId = computed(() => routeTaskId.value || null)

const currentBackgroundRunId = computed<string | null>(() => {
  const value = route.query.runId
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null
})

const backgroundFolders = computed<BackgroundTaskFolder[]>(() =>
  backgroundTaskContainers.value.map((container) => ({
    taskId: container.id,
    name: container.title,
    subtitle: container.subtitle ?? null,
    status: container.status ?? 'idle',
    updatedAt: container.updated_at,
    expanded: expandedBackgroundTaskIds.value.has(container.id),
    runs: (backgroundRunsByTaskId.value[container.id] ?? []).map((session) => ({
      id: session.id,
      title: session.title,
      status: session.status,
      updatedAt: session.updated_at,
      runId: session.run_id,
    })),
  })),
)

const externalFolders = computed<ExternalChannelFolder[]>(() =>
  externalChannelContainers.value.map((container) => ({
    containerId: container.id,
    name: container.title,
    subtitle: container.subtitle ?? null,
    status: container.status ?? null,
    updatedAt: container.updated_at,
    expanded: expandedExternalContainerIds.value.has(container.id),
    sourceChannel: container.source_channel ?? null,
    sessions: (externalSessionsByContainerId.value[container.id] ?? []).map((session) =>
      toSessionItem(session),
    ),
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

function isExternallyManagedSession(sessionId: string): boolean {
  const target =
    workspaceSessions.value.find((item) => item.id === sessionId) ??
    externalFolders.value.flatMap((folder) => folder.sessions).find((item) => item.id === sessionId)
  return !!target?.sourceChannel && target.sourceChannel !== 'workspace'
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
  const fallbackModel =
    referenceSession?.model ?? agentModelById.value.get(fallbackAgentId) ?? 'gpt-5'

  const session = await chatSessionStore.createSession(fallbackAgentId, fallbackModel)
  if (!session) {
    toast.error(chatSessionStore.error || t('chat.createSessionFailed'))
    return
  }

  selectedItemId.value = session.id
  await chatSessionStore.selectSession(session.id)
  await refreshNavigationProjection()
  await router.push({ name: 'workspace-session', params: { sessionId: session.id } })
}

async function onSelectItem(id: string) {
  sidebarMode.value = 'sessions'
  selectedItemId.value = id
  await chatSessionStore.selectSession(id)
  await router.push({ name: 'workspace-session', params: { sessionId: id } })
}

function onSwitchToSessions() {
  sidebarMode.value = 'sessions'
  if (routeTaskId.value) {
    return
  }
  if (selectedItemId.value) {
    void router.push({
      name: 'workspace-session',
      params: { sessionId: selectedItemId.value },
    })
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

async function loadExecutionContainersProjection() {
  executionContainers.value = await listExecutionContainers()
}

async function loadWorkspaceSessionsProjection() {
  const container = workspaceContainer.value
  if (!container) {
    workspaceSessionsById.value = {}
    return
  }

  const sessions = await listExecutionSessions({
    container: {
      kind: 'workspace',
      id: container.id,
    },
  })

  workspaceSessionsById.value = Object.fromEntries(sessions.map((session) => [session.id, session]))
}

async function refreshNavigationProjection() {
  await loadExecutionContainersProjection()
  await loadWorkspaceSessionsProjection()
}

async function loadBackgroundRuns(taskId: string) {
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
}

async function loadExternalSessions(containerId: string) {
  const sessions = await listExecutionSessions({
    container: {
      kind: 'external_channel',
      id: containerId,
    },
  })

  externalSessionsByContainerId.value = {
    ...externalSessionsByContainerId.value,
    [containerId]: sessions,
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
  if (!backgroundRunsByTaskId.value[taskId]) {
    try {
      await loadBackgroundRuns(taskId)
    } catch (error) {
      const message =
        error instanceof Error ? error.message : t('backgroundAgent.runTraceDescription')
      toast.error(message)
    }
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
  if (!externalSessionsByContainerId.value[containerId]) {
    try {
      await loadExternalSessions(containerId)
    } catch (error) {
      const message = error instanceof Error ? error.message : t('workspace.noSessions')
      toast.error(message)
    }
  }
}

async function onSelectBackgroundRun(taskId: string, runId: string | null) {
  const location = {
    name: 'workspace-run',
    params: { taskId },
    query: runId ? { runId } : undefined,
  }

  if (routeTaskId.value === taskId) {
    await router.replace(location)
    return
  }

  await router.push(location)
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
    if (selectedItemId.value === id) {
      selectedItemId.value = null
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
    if (selectedItemId.value === id) {
      selectedItemId.value = null
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
  if (selectedItemId.value === id) {
    await chatSessionStore.selectSession(id)
    await router.push({ name: 'workspace-session', params: { sessionId: id } })
  }
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
    selectedItemId.value = rebuilt.id
    await chatSessionStore.selectSession(rebuilt.id)
    toast.success(t('workspace.session.rebuildSuccess'))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.session.rebuildFailed')
    toast.error(message)
  }
}

function onViewRunTrace(taskId: string) {
  void router.push({
    name: 'workspace-run',
    params: { taskId },
  })
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
      selectedItemId.value = null
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

function externalContainerKey(source: SessionItem['sourceChannel']): string | null {
  switch (source) {
    case 'telegram':
      return 'telegram'
    case 'discord':
      return 'discord'
    case 'slack':
      return 'slack'
    case 'external_legacy':
      return 'external'
    default:
      return null
  }
}

async function ensureExternalContainerForSession(sessionId: string) {
  const loadedContainerId = Object.entries(externalSessionsByContainerId.value).find(([, sessions]) =>
    sessions.some((session) => (session.session_id ?? session.id) === sessionId),
  )?.[0]

  if (loadedContainerId) {
    const next = new Set(expandedExternalContainerIds.value)
    next.add(loadedContainerId)
    expandedExternalContainerIds.value = next
    return
  }

  const currentSession = chatSessionStore.currentSession
  const source = currentSession?.source_channel
  const key = externalContainerKey(source)
  if (!currentSession || !key) {
    return
  }

  const conversationId = currentSession.source_conversation_id?.trim() || currentSession.id
  const containerId = `${key}:${conversationId}`
  const next = new Set(expandedExternalContainerIds.value)
  next.add(containerId)
  expandedExternalContainerIds.value = next

  if (!externalSessionsByContainerId.value[containerId]) {
    try {
      await loadExternalSessions(containerId)
    } catch (error) {
      console.warn('Failed to load external container sessions:', error)
    }
  }
}

watch(
  () => chatSessionStore.currentSessionId,
  (newId) => {
    if (newId && !routeTaskId.value) {
      selectedItemId.value = newId
    }
  },
)

watch([selectedItemId, routeTaskId], () => {
  toolPanel.clearHistory()
})

watch(
  [routeSessionId, routeTaskId],
  async ([sessionId, taskId]) => {
    if (sidebarMode.value !== 'agents') {
      sidebarMode.value = 'sessions'
    }

    if (taskId) {
      selectedItemId.value = null
      const next = new Set(expandedBackgroundTaskIds.value)
      next.add(taskId)
      expandedBackgroundTaskIds.value = next

      if (!backgroundRunsByTaskId.value[taskId]) {
        try {
          await loadBackgroundRuns(taskId)
        } catch (error) {
          console.warn('Failed to load background runs for task route:', error)
        }
      }
      return
    }

    if (sessionId) {
      selectedItemId.value = sessionId
      await chatSessionStore.selectSession(sessionId)
      await ensureExternalContainerForSession(sessionId)
      return
    }

    selectedItemId.value = chatSessionStore.currentSessionId
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
          :workspace-sessions="workspaceSessions"
          :current-session-id="selectedItemId"
          :background-folders="backgroundFolders"
          :external-folders="externalFolders"
          :current-background-task-id="currentBackgroundTaskId"
          :current-background-run-id="currentBackgroundRunId"
          class="flex-1 min-h-0"
          @select="onSelectItem"
          @new-session="onNewSession"
          @rename="onRenameSession"
          @archive="onArchiveSession"
          @delete="onDeleteSession"
          @convert-to-background-agent="onConvertToBackgroundAgent"
          @convert-to-workspace-session="onConvertToWorkspaceSession"
          @view-run-trace="onViewRunTrace"
          @rebuild="onRebuildSession"
          @toggle-background-task="onToggleBackgroundTask"
          @select-background-run="onSelectBackgroundRun"
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

      <WorkspaceRunPanel
        v-if="sidebarMode === 'sessions' && routeTaskId"
        :task-id="routeTaskId"
        :selected-run-id="currentBackgroundRunId"
        class="flex-1 min-w-0"
        @refresh="refreshNavigationProjection"
        @select-run="onSelectBackgroundRun(routeTaskId, $event)"
        @select-thread-item="onThreadSelection"
      />

      <ChatPanel
        v-else-if="sidebarMode === 'sessions'"
        class="flex-1 min-w-0"
        @show-panel="onShowPanel"
        @tool-result="onToolResult"
        @thread-selection="onThreadSelection"
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
