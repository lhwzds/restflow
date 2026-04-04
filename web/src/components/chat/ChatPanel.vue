<script setup lang="ts">
/**
 * ChatPanel Component
 *
 * Main chat area combining header, message list, and input.
 * Integrates with useChatStream for real-time streaming and
 * detects show_panel tool calls for Canvas panel display.
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Play, Pause, RotateCcw, XCircle, Activity, ChevronRight, GitBranch } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import MessageList from './MessageList.vue'
import ExecutionStatusBar from './ExecutionStatusBar.vue'
import ChatBox from '@/components/workspace/ChatBox.vue'
import TaskStatusBadge from '@/components/task/TaskStatusBadge.vue'
import { useChatSession } from '@/composables/workspace/useChatSession'
import { useChatStream, type StreamStep } from '@/composables/workspace/useChatStream'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useTaskStore } from '@/stores/taskStore'
import { useModelsStore } from '@/stores/modelsStore'
import { listAgents, getAgent, updateAgent } from '@/api/agents'
import { BackendError } from '@/api/http-client'
import { steerChatStream } from '@/api/chat-stream'
import {
  sendChatMessage as sendChatMessageApi,
  subscribeSessionEvents,
  type UnlistenFn,
} from '@/api/chat-session'
import {
  getExecutionRunThread,
  listExecutionContainers,
  listRuns,
} from '@/api/execution-console'
import { useToast } from '@/composables/useToast'
import type { AgentFile, ModelOption } from '@/types/workspace'
import type { ModelId } from '@/types/generated/ModelId'
import type { VoiceMessageInfo } from '@/composables/workspace/useVoiceRecorder'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { ExecutionContainerKind } from '@/types/generated/ExecutionContainerKind'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import type { RunSummary } from '@/types/generated/RunSummary'
import type { ThreadSelection } from './threadItems'
import {
  buildRunThreadItems,
  buildTranscriptThreadItems,
  type ThreadItem,
} from './threadItems'
import { buildVoiceMessageContent } from './voiceMessageContent'

const props = withDefaults(
  defineProps<{
    selectedRunId?: string | null
    containerId?: string | null
    backgroundTaskId?: string | null
    autoSelectRecent?: boolean
  }>(),
  {
    selectedRunId: null,
    containerId: null,
    backgroundTaskId: null,
    autoSelectRecent: true,
  },
)

const emit = defineEmits<{
  showPanel: [resultJson: string]
  toolResult: [step: StreamStep]
  threadSelection: [selection: ThreadSelection]
  threadLoaded: [thread: ExecutionThread | null]
  runStarted: [payload: { containerId: string; runId: string }]
}>()

const toast = useToast()
const { t } = useI18n()
const router = useRouter()
const chatSessionStore = useChatSessionStore()
const taskStore = useTaskStore()
const modelsStore = useModelsStore()

// Track last user message content for regeneration
const lastUserContent = ref('')

// Map voice message file paths to blob URLs for audio playback
const voiceAudioUrls = ref<Map<string, { blobUrl: string; duration: number }>>(new Map())

const {
  currentSession,
  messages: chatMessages,
  isSending,
  createSession: createChatSession,
} = useChatSession({ autoLoad: true, autoSelectRecent: props.autoSelectRecent })

// Chat stream for real-time responses
const chatStream = useChatStream(() => chatSessionStore.currentSessionId)

const selectedAgent = ref<string | null>(null)
const selectedModel = ref('')
const availableAgents = ref<AgentFile[]>([])
const availableModels = computed<ModelOption[]>(() =>
  modelsStore.getAllModels.map((model) => ({
    id: model.model,
    name: model.name,
    provider: model.provider,
  })),
)
const chatBoxKey = computed(() => {
  const sessionId = currentSession.value?.id ?? 'new'
  return `chatbox-${sessionId}-${availableAgents.value.length}-${availableModels.value.length}`
})

// Messages from store
const messages = computed<ChatMessage[]>(() => chatMessages.value)
const executionThread = ref<ExecutionThread | null>(null)
let executionThreadLoadVersion = 0
const pendingRunId = ref<string | null>(null)
const pendingStreamResetRunId = ref<string | null>(null)
let persistedRunRetryTimer: number | null = null
let persistedRunRetryDeadline = 0
const containerRunSummaries = ref<RunSummary[]>([])
let containerRunSummariesLoadVersion = 0

// Stream state
const isStreaming = computed(() => chatStream.isStreaming.value)
const isExecuting = computed(() => isSending.value || isStreaming.value)
const streamContent = computed(() => chatStream.state.value.content)
const streamThinking = computed(() => chatStream.state.value.thinking)
const streamSteps = computed(() => chatStream.state.value.steps)
const executionStartedAt = ref<number | null>(null)
const executionStatusLabel = computed(() => {
  if (streamThinking.value.trim()) {
    return 'Thinking...'
  }
  if (isExecuting.value && !isStreaming.value) {
    return 'Preparing run...'
  }
  return null
})
const activeStreamRunId = computed(() => chatStream.state.value.messageId)
const centerRunId = computed(() => props.selectedRunId || pendingRunId.value || null)
const isCenterRunStreaming = computed(
  () => !!centerRunId.value && centerRunId.value === activeStreamRunId.value,
)
const resolvedContainerId = computed(
  () => props.containerId || executionThread.value?.focus.container_id || chatSessionStore.currentSessionId || null,
)

function isOptimisticMessage(message: ChatMessage): boolean {
  return message.id.startsWith('optimistic-')
}

function messageTimestampMs(message: ChatMessage): number | null {
  const value = Number(message.timestamp)
  if (!Number.isFinite(value) || value <= 0) return null
  return value
}

interface RunMessageBoundary {
  runId: string
  startedAt: number
  updatedAt: number
  endedAt: number
}

function summaryToRunMessageBoundary(summary: RunSummary): RunMessageBoundary | null {
  if (!summary.run_id) return null

  return {
    runId: summary.run_id,
    startedAt: Number(summary.started_at ?? 0),
    updatedAt: Number(summary.updated_at ?? 0),
    endedAt: Number(summary.ended_at ?? 0),
  }
}

function threadToRunMessageBoundary(thread: ExecutionThread | null): RunMessageBoundary | null {
  if (!thread?.focus.run_id) return null

  return {
    runId: thread.focus.run_id,
    startedAt: Number(thread.focus.started_at ?? 0),
    updatedAt: Number(thread.focus.updated_at ?? 0),
    endedAt: Number(thread.focus.ended_at ?? 0),
  }
}

function runBoundarySortTime(boundary: RunMessageBoundary): number {
  if (boundary.startedAt > 0) return boundary.startedAt
  return Math.max(boundary.updatedAt, boundary.endedAt, 0)
}

function runBoundaryEndTime(boundary: RunMessageBoundary): number {
  return Math.max(boundary.endedAt, boundary.updatedAt, boundary.startedAt, 0)
}

function sortRunMessageBoundaries(boundaries: RunMessageBoundary[]): RunMessageBoundary[] {
  return [...boundaries].sort((left, right) => {
    const delta = runBoundarySortTime(left) - runBoundarySortTime(right)
    if (delta !== 0) return delta
    return left.runId.localeCompare(right.runId)
  })
}

function deriveContainerMessageWindow(
  runId: string,
  boundaries: RunMessageBoundary[],
): { lowerBound: number; upperBound: number } | null {
  const sorted = sortRunMessageBoundaries(boundaries)
  if (sorted.length === 0) {
    return { lowerBound: Number.NEGATIVE_INFINITY, upperBound: Number.POSITIVE_INFINITY }
  }

  const currentIndex = sorted.findIndex((boundary) => boundary.runId === runId)
  if (currentIndex >= 0) {
    const previous = currentIndex > 0 ? sorted[currentIndex - 1] : null
    const next = currentIndex < sorted.length - 1 ? sorted[currentIndex + 1] : null
    return {
      lowerBound: previous ? runBoundaryEndTime(previous) : Number.NEGATIVE_INFINITY,
      upperBound: next
        ? next.startedAt > 0
          ? next.startedAt
          : runBoundarySortTime(next)
        : Number.POSITIVE_INFINITY,
    }
  }

  if (pendingRunId.value === runId) {
    const previous = sorted[sorted.length - 1] ?? null
    return {
      lowerBound: previous ? runBoundaryEndTime(previous) : Number.NEGATIVE_INFINITY,
      upperBound: Number.POSITIVE_INFINITY,
    }
  }

  return null
}

function deriveThreadHeuristicMessageWindow(
  thread: ExecutionThread,
): { lowerBound: number; upperBound: number } {
  const boundary = threadToRunMessageBoundary(thread)
  const lowerBound = boundary?.startedAt && boundary.startedAt > 0 ? boundary.startedAt : 0
  const upperCandidate = boundary ? runBoundaryEndTime(boundary) : 0
  return {
    lowerBound,
    upperBound: upperCandidate > 0 ? upperCandidate : Number.POSITIVE_INFINITY,
  }
}

async function loadContainerRunSummaries(
  containerId: string | null,
  kind: ExecutionContainerKind | null,
): Promise<RunSummary[]> {
  const version = ++containerRunSummariesLoadVersion

  if (!containerId || !kind) {
    containerRunSummaries.value = []
    return []
  }

  try {
    const runs = await listRuns({
      container: {
        kind,
        id: containerId,
      },
    })
    if (version !== containerRunSummariesLoadVersion) return containerRunSummaries.value
    containerRunSummaries.value = runs.filter((summary) => !!summary.run_id)
    return containerRunSummaries.value
  } catch (error) {
    if (version !== containerRunSummariesLoadVersion) return containerRunSummaries.value
    console.warn('Failed to load container run summaries:', error)
    containerRunSummaries.value = []
    return []
  }
}

function buildRunScopedMessages(
  allMessages: ChatMessage[],
  thread: ExecutionThread | null,
  runId: string | null,
  includeOptimisticMessages: boolean,
  runSummaries: RunSummary[],
): ChatMessage[] {
  if (!runId) return allMessages

  const optimisticMessages = includeOptimisticMessages
    ? allMessages.filter(isOptimisticMessage)
    : []

  if (!thread || thread.focus.run_id !== runId) {
    return optimisticMessages
  }

  const boundaries = new Map<string, RunMessageBoundary>()
  for (const summary of runSummaries) {
    const boundary = summaryToRunMessageBoundary(summary)
    if (boundary) {
      boundaries.set(boundary.runId, boundary)
    }
  }
  const threadBoundary = threadToRunMessageBoundary(thread)
  if (threadBoundary) {
    boundaries.set(threadBoundary.runId, threadBoundary)
  }

  const containerWindow = deriveContainerMessageWindow(runId, [...boundaries.values()])
  const hasAdjacentBoundary =
    !!containerWindow &&
    (Number.isFinite(containerWindow.lowerBound) || Number.isFinite(containerWindow.upperBound))
  const messageWindow = hasAdjacentBoundary
    ? containerWindow
    : deriveThreadHeuristicMessageWindow(thread)

  const persistedMessages = allMessages.filter((message) => {
    if (isOptimisticMessage(message)) return false
    const timestamp = messageTimestampMs(message)
    if (timestamp == null) return false
    if (hasAdjacentBoundary) {
      return timestamp > messageWindow.lowerBound && timestamp < messageWindow.upperBound
    }
    return timestamp >= messageWindow.lowerBound && timestamp <= messageWindow.upperBound
  })

  return [...persistedMessages, ...optimisticMessages]
}

const runScopedMessages = computed<ChatMessage[]>(() =>
  buildRunScopedMessages(
    messages.value,
    executionThread.value,
    centerRunId.value,
    isCenterRunStreaming.value,
    containerRunSummaries.value,
  ),
)

const threadItems = computed<ThreadItem[]>(() => {
  if (centerRunId.value) {
    return buildRunThreadItems({
      thread:
        executionThread.value?.focus.run_id === centerRunId.value ? executionThread.value : null,
      messages: runScopedMessages.value,
      steps: isCenterRunStreaming.value ? streamSteps.value : [],
      streamContent: isCenterRunStreaming.value ? streamContent.value : '',
    })
  }

  return buildTranscriptThreadItems({
    messages: messages.value,
    steps: streamSteps.value,
    streamContent: streamContent.value,
  })
})

// Token stats from stream
const inputTokens = computed(() => chatStream.state.value.inputTokens)
const outputTokens = computed(() => chatStream.state.value.outputTokens)
const totalTokens = computed(() => chatStream.state.value.tokenCount)
const tokensPerSecond = computed(() => chatStream.tokensPerSecond.value)
const durationMs = computed(() => chatStream.duration.value)

// Task linked to current session (if any)
const linkedTask = computed(() => {
  if (props.backgroundTaskId) {
    return taskStore?.tasks?.find((task) => task.id === props.backgroundTaskId) ?? null
  }
  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return null
  return taskStore?.taskBySessionId?.(sessionId) ?? null
})
interface RunBreadcrumbNode {
  key: 'root' | 'parent' | 'current'
  runId: string
  label: string
  badge: string
  clickable: boolean
}

const executionFocus = computed(() => executionThread.value?.focus ?? null)
const breadcrumbNodes = ref<RunBreadcrumbNode[]>([])
let breadcrumbLoadVersion = 0
const showRunBreadcrumb = computed(() => breadcrumbNodes.value.length > 1)
const currentRunAgentName = computed(() => {
  const agentId = executionFocus.value?.agent_id
  if (!agentId) return null
  return availableAgents.value.find((agent) => agent.id === agentId)?.name ?? agentId
})
const isExternalSessionManaged = computed(() => {
  const source = currentSession.value?.source_channel
  return !!source && source !== 'workspace'
})
const activeContainerKind = computed<ExecutionContainerKind | null>(() => {
  if (props.backgroundTaskId) return 'background_task'
  if (isExternalSessionManaged.value) return 'external_channel'
  return resolvedContainerId.value ? 'workspace' : null
})
const taskCanPause = computed(() => linkedTask.value?.status === 'active')
const taskCanResume = computed(() => linkedTask.value?.status === 'paused')
const taskCanRun = computed(
  () => linkedTask.value?.status === 'active' || linkedTask.value?.status === 'paused',
)
const taskCanStop = computed(() => linkedTask.value?.status === 'running')

async function handleTaskPause() {
  if (!linkedTask.value) return
  await taskStore?.pauseTask?.(linkedTask.value.id)
}

async function handleTaskResume() {
  if (!linkedTask.value) return
  await taskStore?.resumeTask?.(linkedTask.value.id)
}

async function handleTaskRun() {
  if (!linkedTask.value) return
  await taskStore?.runTaskNow?.(linkedTask.value.id)
}

async function handleTaskStop() {
  if (!linkedTask.value) return
  await taskStore?.stopTask?.(linkedTask.value.id)
}

async function handleOpenRunTrace() {
  const runId = props.selectedRunId || executionThread.value?.focus.run_id || null
  const containerId =
    executionThread.value?.focus.container_id ||
    props.backgroundTaskId ||
    linkedTask.value?.id ||
    chatSessionStore.currentSessionId ||
    null
  if (runId) {
    await router.push({
      name: containerId ? 'workspace-container-run' : 'workspace-run-id',
      params: containerId ? { containerId, runId } : { runId },
    })
    return
  }

  if (!linkedTask.value) return

  try {
    const runs = await listRuns({
      container: {
        kind: 'background_task',
        id: linkedTask.value.id,
      },
    })
    const latestRunId = runs.find((entry) => !!entry.run_id)?.run_id ?? null
    const latestRunContainerId =
      runs.find((entry) => !!entry.run_id)?.container_id ?? linkedTask.value.id
    if (latestRunId) {
      await router.push({
        name: 'workspace-container-run',
        params: { containerId: latestRunContainerId, runId: latestRunId },
      })
      return
    }
  } catch (error) {
    console.warn('Failed to resolve latest background run for trace view:', error)
  }

  await router.push({
    name: 'workspace-container',
    params: { containerId: linkedTask.value.id },
  })
}

async function navigateToBreadcrumbRun(runId: string) {
  const containerId = executionFocus.value?.container_id ?? null
  if (!containerId) return

  await router.push({
    name: 'workspace-container-run',
    params: { containerId, runId },
  })
}

watch(
  executionFocus,
  async (focus) => {
    const version = ++breadcrumbLoadVersion

    if (focus?.kind !== 'subagent_run' || !focus.run_id) {
      breadcrumbNodes.value = []
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
          const thread = await getExecutionRunThread(runId)
          runLabels.set(runId, thread.focus.title || runId)
        } catch {
          runLabels.set(runId, runId)
        }
      }),
    )

    if (version !== breadcrumbLoadVersion) return

    const nodes: RunBreadcrumbNode[] = []
    if (focus.root_run_id && focus.root_run_id !== focus.run_id) {
      nodes.push({
        key: 'root',
        runId: focus.root_run_id,
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
        label: runLabels.get(focus.parent_run_id) ?? 'Parent run',
        badge: 'Parent',
        clickable: true,
      })
    }
    nodes.push({
      key: 'current',
      runId: focus.run_id,
      label: runLabels.get(focus.run_id) ?? focus.run_id,
      badge: 'Child',
      clickable: false,
    })

    breadcrumbNodes.value = nodes
  },
  { immediate: true },
)

function onSelectThreadItem(selection: ThreadSelection) {
  emit('threadSelection', selection)
}

// Track processed tool call IDs to avoid duplicate emits
const processedToolIds = ref<Set<string>>(new Set())

// Finished tool steps (computed avoids deep-watching entire steps array)
const finishedToolSteps = computed(() =>
  chatStream.state.value.steps.filter(
    (s) =>
      s.type === 'tool_call' &&
      (s.status === 'completed' || s.status === 'failed') &&
      s.result &&
      s.toolId,
  ),
)

// Watch only when new tool steps complete (by length change)
watch(
  () => finishedToolSteps.value.length,
  () => {
    for (const step of finishedToolSteps.value) {
      if (!processedToolIds.value.has(step.toolId!)) {
        processedToolIds.value.add(step.toolId!)
        emit('toolResult', step)
        if (step.name === 'show_panel') {
          emit('showPanel', step.result!)
        }
      }
    }
  },
)

// Sync agent/model from current session and reset stream only when session id changes
watch(
  () => ({
    id: currentSession.value?.id ?? null,
    agentId: currentSession.value?.agent_id ?? null,
    model: currentSession.value?.model ?? '',
  }),
  (next, prev) => {
    if (next.id !== prev?.id) {
      chatStream.reset()
      processedToolIds.value.clear()
    }

    if (next.id) {
      selectedAgent.value = next.agentId
      selectedModel.value = next.model
    }
  },
  { immediate: true },
)

watch(
  () => props.selectedRunId,
  (runId) => {
    if (!runId) return
    if (pendingRunId.value === runId) {
      pendingRunId.value = null
      return
    }
    pendingRunId.value = null
  },
)

watch(
  [resolvedContainerId, activeContainerKind],
  ([containerId, kind]) => {
    void loadContainerRunSummaries(containerId, kind)
  },
  { immediate: true },
)

watch(
  centerRunId,
  (runId, previousRunId) => {
    if (previousRunId && previousRunId !== runId && pendingStreamResetRunId.value === previousRunId) {
      pendingStreamResetRunId.value = null
      clearPersistedRunRetry()
    }
  },
)

function clearPersistedRunRetry() {
  if (persistedRunRetryTimer != null) {
    window.clearTimeout(persistedRunRetryTimer)
    persistedRunRetryTimer = null
  }
  persistedRunRetryDeadline = 0
}

function finalizePersistedRun(runId: string) {
  if (pendingStreamResetRunId.value === runId) {
    pendingStreamResetRunId.value = null
    chatStream.reset()
  }
  clearPersistedRunRetry()
}

async function loadExecutionThreadForRun(
  runId: string | null,
  options: { allowNotFound?: boolean } = {},
): Promise<ExecutionThread | null> {
  const requestVersion = ++executionThreadLoadVersion

  if (!runId) {
    executionThread.value = null
    emit('threadLoaded', null)
    return null
  }

  if (executionThread.value?.focus.run_id !== runId) {
    executionThread.value = null
    emit('threadLoaded', null)
  }

  try {
    const thread = await getExecutionRunThread(runId)

    if (requestVersion !== executionThreadLoadVersion) return executionThread.value
    executionThread.value = thread
    emit('threadLoaded', thread)
    return thread
  } catch (error) {
    if (requestVersion !== executionThreadLoadVersion) return executionThread.value

    if (error instanceof BackendError && error.code === 404 && options.allowNotFound) {
      return null
    }

    if (error instanceof BackendError && error.code === 404) {
      executionThread.value = null
      emit('threadLoaded', null)
      return null
    }

    console.warn('Failed to load execution thread for run:', error)
    executionThread.value = null
    emit('threadLoaded', null)
    return null
  }
}

function schedulePersistedRunRetry(runId: string, delayMs = 250) {
  if (persistedRunRetryTimer != null) {
    window.clearTimeout(persistedRunRetryTimer)
  }

  persistedRunRetryTimer = window.setTimeout(async () => {
    persistedRunRetryTimer = null

    if (centerRunId.value !== runId) {
      pendingStreamResetRunId.value = null
      clearPersistedRunRetry()
      return
    }

    const thread = await loadExecutionThreadForRun(runId, { allowNotFound: true })
    if (thread) {
      finalizePersistedRun(runId)
      return
    }

    if (Date.now() >= persistedRunRetryDeadline) {
      pendingStreamResetRunId.value = null
      clearPersistedRunRetry()
      return
    }

    schedulePersistedRunRetry(runId, Math.min(delayMs * 2, 1000))
  }, delayMs)
}

async function navigateToLatestContainerRun(sessionId: string) {
  if (props.selectedRunId) return

  try {
    const containers = await listExecutionContainers()
    const container =
      containers.find((entry) => entry.id === sessionId || entry.latest_session_id === sessionId) ?? null
    if (!container) return

    if (container.latest_run_id) {
      await router.replace({
        name: 'workspace-container-run',
        params: {
          containerId: container.id,
          runId: container.latest_run_id,
        },
      })
      return
    }

    const runs = await listRuns({
      container: {
        kind: container.kind,
        id: container.id,
      },
    })
    const latestRun = runs.find((entry) => !!entry.run_id)
    if (!latestRun?.run_id) return

    await router.replace({
      name: 'workspace-container-run',
      params: {
        containerId: latestRun.container_id || container.id,
        runId: latestRun.run_id,
      },
    })
  } catch (error) {
    console.warn('Failed to resolve latest container run:', error)
  }
}

watch(
  () => centerRunId.value,
  (runId) => {
    void loadExecutionThreadForRun(runId, {
      allowNotFound: !!runId && (runId === pendingRunId.value || runId === activeStreamRunId.value),
    })
  },
  { immediate: true },
)

async function syncSessionFromBackend() {
  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return

  const runId = centerRunId.value ?? activeStreamRunId.value
  const refreshed = await chatSessionStore.refreshSession(sessionId)
  if (refreshed) {
    chatSessionStore.updateSessionLocally(refreshed)
    await loadContainerRunSummaries(resolvedContainerId.value, activeContainerKind.value)

    if (runId) {
      const thread = await loadExecutionThreadForRun(runId, { allowNotFound: true })
      if (thread) {
        finalizePersistedRun(runId)
        return
      }

      if (runId === pendingRunId.value || runId === activeStreamRunId.value) {
        pendingStreamResetRunId.value = runId
        persistedRunRetryDeadline = Date.now() + 10000
        schedulePersistedRunRetry(runId)
        return
      }
    } else {
      await navigateToLatestContainerRun(sessionId)
      chatStream.reset()
      return
    }
  }
}

watch(isStreaming, (streaming, prevStreaming) => {
  if (prevStreaming && !streaming) {
    void syncSessionFromBackend()
  }
})

watch(
  isExecuting,
  (executing, previousExecuting) => {
    if (executing && !previousExecuting) {
      executionStartedAt.value = Date.now()
      return
    }
    if (!executing) {
      executionStartedAt.value = null
    }
  },
  { immediate: true },
)

async function sendMessageWithStream(message: string) {
  chatStream.reset()
  processedToolIds.value.clear()

  // Optimistically show the user message immediately
  const session = chatSessionStore.currentSession
  if (session) {
    session.messages.push({
      id: `optimistic-${Date.now()}`,
      role: 'user',
      content: message,
      timestamp: 0n,
      execution: null,
    })
  }

  try {
    const streamId = await chatStream.send(message)
    const containerId = resolvedContainerId.value
    if (containerId) {
      pendingRunId.value = streamId
      emit('runStarted', { containerId, runId: streamId })
    }
    // Session sync is handled by the isStreaming watcher when streaming ends.
    // Do NOT call syncSessionFromBackend() here — send() returns before
    // the stream completes, so an early sync would fetch stale data.
  } catch (error) {
    const messageText = error instanceof Error ? error.message : t('chat.sendMessageFailed')
    toast.error(messageText)
  }
}

async function loadAgents() {
  try {
    const agents = await listAgents()
    availableAgents.value = agents.map((agent) => ({
      id: agent.id,
      name: agent.name,
      path: `agents/${agent.id}`,
    }))

    if (!selectedAgent.value && availableAgents.value.length > 0) {
      selectedAgent.value = availableAgents.value[0]?.id ?? null
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadAgentsFailed')
    toast.error(message)
  }
}

async function loadModels() {
  try {
    await modelsStore.loadModels()

    if (!selectedModel.value && availableModels.value.length > 0) {
      selectedModel.value = availableModels.value[0]?.id ?? ''
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadModelsFailed')
    toast.error(message)
  }
}

async function ensureChatSession(): Promise<boolean> {
  if (chatSessionStore.currentSessionId) {
    return true
  }

  if (!selectedAgent.value) {
    toast.error(t('chat.selectAgentToStart'))
    return false
  }

  if (!selectedModel.value) {
    toast.error(t('chat.selectModelToStart'))
    return false
  }

  const session = await createChatSession(selectedAgent.value, selectedModel.value)
  if (!session) {
    toast.error(t('chat.createSessionFailed'))
    return false
  }

  return true
}

async function onSendMessage(message: string) {
  const canSend = await ensureChatSession()
  if (!canSend) return

  if (isExecuting.value) {
    const sessionId = chatSessionStore.currentSessionId
    if (sessionId) {
      try {
        const steered = await steerChatStream(sessionId, message)
        if (steered) {
          // Persist the steering instruction in session history.
          const session = await sendChatMessageApi(sessionId, message)
          chatSessionStore.updateSessionLocally(session)
          return
        }
      } catch (error) {
        toast.error(error instanceof Error ? error.message : t('chat.sendMessageFailed'))
        return
      }
    }

    // No active steerable stream: fall back to starting a new streamed turn.
    lastUserContent.value = message
    await sendMessageWithStream(message)
    return
  }

  lastUserContent.value = message
  await sendMessageWithStream(message)

  if (chatSessionStore.error) {
    toast.error(chatSessionStore.error)
  }
}

async function onUpdateSelectedAgent(agentId: string | null) {
  const oldAgent = selectedAgent.value
  selectedAgent.value = agentId

  if (!agentId) return

  const session = currentSession.value
  if (!session || session.agent_id === agentId) return
  if (isExternalSessionManaged.value) {
    selectedAgent.value = oldAgent
    toast.error(t('workspace.session.managedExternally'))
    return
  }

  const updated = await chatSessionStore.updateSessionAgent(session.id, agentId)
  if (!updated) {
    selectedAgent.value = oldAgent
    toast.error(t('chat.updateSessionAgentFailed'))
    return
  }

  selectedAgent.value = updated.agent_id
}

async function onUpdateSelectedModel(model: string) {
  const oldModel = selectedModel.value
  selectedModel.value = model

  const session = currentSession.value
  if (!session || session.model === model) return
  if (isExternalSessionManaged.value) {
    selectedModel.value = oldModel
    toast.error(t('workspace.session.managedExternally'))
    return
  }

  const updated = await chatSessionStore.updateSessionModel(session.id, model)
  if (!updated) {
    selectedModel.value = oldModel
    toast.error(t('chat.updateSessionModelFailed'))
    return
  }

  selectedModel.value = updated.model

  // Also persist the model to the agent's default so future sessions use it
  const agentId = session.agent_id
  if (agentId) {
    try {
      const stored = await getAgent(agentId)
      const nextModel = model as ModelId
      const metadata = modelsStore.getModelMetadata(nextModel)
      const resolvedProvider = metadata?.provider ?? stored.agent.model_ref?.provider
      const request = {
        agent: {
          ...stored.agent,
          model: nextModel,
          model_ref: resolvedProvider
            ? {
                provider: resolvedProvider,
                model: nextModel,
              }
            : undefined,
        },
      }
      await updateAgent(agentId, request)
    } catch {
      // Non-critical: session model was updated, agent default is best-effort
    }
  }
}

async function handleCancel() {
  await chatStream.cancel()
}

async function handleRegenerate() {
  if (!lastUserContent.value || isExecuting.value) return

  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return

  await sendMessageWithStream(lastUserContent.value)

  if (chatSessionStore.error) {
    toast.error(chatSessionStore.error)
  }
}

function getSessionId(): string | undefined {
  return chatSessionStore.currentSessionId ?? undefined
}

async function onSendVoiceMessage(info: VoiceMessageInfo) {
  const message = buildVoiceMessageContent(info.filePath)

  // Cache the blob URL for audio playback in the UI
  voiceAudioUrls.value.set(info.filePath, {
    blobUrl: info.audioBlobUrl,
    duration: info.durationSec,
  })

  const canSend = await ensureChatSession()
  if (!canSend) return
  await sendMessageWithStream(message)
}

function onViewToolResult(step: StreamStep) {
  if (step.result) {
    emit('toolResult', step)
    if (step.name === 'show_panel') {
      emit('showPanel', step.result)
    }
  }
}

// Subscribe to daemon session events (e.g. Telegram messages)
let unlistenSessionEvents: UnlistenFn | null = null

onMounted(async () => {
  loadAgents()
  loadModels()
  await taskStore?.fetchTasks?.()

  try {
    unlistenSessionEvents = await subscribeSessionEvents((event) => {
      if (event.type === 'MessageAdded' || event.type === 'Updated') {
        const sessionId = event.session_id
        // Refresh if it's the currently viewed session
        if (sessionId === chatSessionStore.currentSessionId) {
          void chatSessionStore.refreshSession(sessionId).then((session) => {
            if (session) {
              chatSessionStore.updateSessionLocally(session)
            }
            void loadContainerRunSummaries(resolvedContainerId.value, activeContainerKind.value)
            const runId = centerRunId.value
            if (runId) {
              return loadExecutionThreadForRun(runId, { allowNotFound: true }).then(() => undefined)
            }
            return navigateToLatestContainerRun(sessionId)
          })
        }
        // Also refresh summaries so the sidebar stays up to date
        chatSessionStore.fetchSummaries()
      }
    })
  } catch {
    // Non-critical: session events just won't auto-refresh
  }
})

onUnmounted(() => {
  unlistenSessionEvents?.()
  clearPersistedRunRetry()
})

// Expose for parent (Workspace.vue needs session list data)
defineExpose({
  selectedAgent,
  availableAgents,
  isSending,
})
</script>

<template>
  <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
    <!-- Header: Task control bar or plain drag region -->
    <div
      v-if="linkedTask"
      class="flex items-center gap-2 px-3 py-1.5 border-b border-border shrink-0 text-xs text-muted-foreground"
    >
      <TaskStatusBadge :status="linkedTask.status" />
      <Button
        variant="ghost"
        size="sm"
        class="h-6 gap-1 px-2 text-xs"
        data-testid="open-run-trace"
        :title="t('task.openRunTrace')"
        @click="handleOpenRunTrace"
      >
        <Activity :size="12" />
        <span>{{ t('task.openRunTrace') }}</span>
      </Button>
      <div class="flex-1" />
      <Button
        v-if="taskCanPause"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('task.pause')"
        @click="handleTaskPause"
      >
        <Pause :size="12" />
      </Button>
      <Button
        v-if="taskCanResume"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('task.resume')"
        @click="handleTaskResume"
      >
        <RotateCcw :size="12" />
      </Button>
      <Button
        v-if="taskCanRun"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('task.runNow')"
        @click="handleTaskRun"
      >
        <Play :size="12" />
      </Button>
      <Button
        v-if="taskCanStop"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('task.stop')"
        @click="handleTaskStop"
      >
        <XCircle :size="12" />
      </Button>
    </div>
    <div v-else class="h-8 shrink-0" />

    <div
      v-if="showRunBreadcrumb"
      class="flex items-center gap-1.5 border-b border-border px-3 py-1 text-[11px] text-muted-foreground"
      data-testid="run-breadcrumb"
    >
      <GitBranch :size="11" class="shrink-0 text-muted-foreground/80" />
      <template v-for="(node, index) in breadcrumbNodes" :key="`${node.key}-${node.runId}`">
        <ChevronRight
          v-if="index > 0"
          :size="11"
          class="shrink-0 text-muted-foreground/70"
        />
        <span
          class="rounded-sm border border-border/60 bg-muted/50 px-1 py-0 text-[8px] font-medium uppercase tracking-[0.08em]"
        >
          {{ node.badge }}
        </span>
        <Button
          v-if="node.clickable"
          variant="ghost"
          size="sm"
          class="h-5 gap-1 px-1.5 text-[11px] text-muted-foreground"
          :data-testid="`run-breadcrumb-node-${node.key}`"
          @click="navigateToBreadcrumbRun(node.runId)"
        >
          <span>{{ node.label }}</span>
        </Button>
        <span
          v-else
          class="truncate font-medium text-foreground/85"
          data-testid="run-breadcrumb-current"
        >
          {{ node.label }}
        </span>
      </template>
      <span v-if="currentRunAgentName" class="truncate text-muted-foreground/85">
        · {{ currentRunAgentName }}
      </span>
    </div>

    <!-- Execution status bar -->
    <ExecutionStatusBar
      v-if="isExecuting"
      :is-active="isExecuting"
      :started-at="executionStartedAt"
      :steps="streamSteps"
      :fallback-label="executionStatusLabel"
    />

    <!-- Message List -->
    <MessageList
      :messages="messages"
      :is-streaming="isStreaming"
      :stream-content="streamContent"
      :stream-thinking="streamThinking"
      :steps="streamSteps"
      :thread-items="threadItems"
      :voice-audio-urls="voiceAudioUrls"
      @view-tool-result="onViewToolResult"
      @select-thread-item="onSelectThreadItem"
      @regenerate="handleRegenerate"
    />

    <!-- Input Area -->
    <div class="shrink-0 px-4 pb-4">
      <ChatBox
        :key="chatBoxKey"
        :is-expanded="true"
        :is-executing="isExecuting"
        :selected-agent="selectedAgent"
        :selected-model="selectedModel"
        :available-agents="availableAgents"
        :available-models="availableModels"
        :is-streaming="isStreaming"
        :input-tokens="inputTokens"
        :output-tokens="outputTokens"
        :total-tokens="totalTokens"
        :tokens-per-second="tokensPerSecond"
        :duration-ms="durationMs"
        :session-locked="isExternalSessionManaged"
        :get-session-id="getSessionId"
        @send="onSendMessage"
        @send-voice-message="onSendVoiceMessage"
        @cancel="handleCancel"
        @close="() => {}"
        @update:selected-agent="onUpdateSelectedAgent"
        @update:selected-model="onUpdateSelectedModel"
      />
    </div>
  </div>
</template>
