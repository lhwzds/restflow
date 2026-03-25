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
import { Play, Pause, RotateCcw, XCircle, Activity } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import MessageList from './MessageList.vue'
import ChatBox from '@/components/workspace/ChatBox.vue'
import AgentStatusBadge from '@/components/background-agent/AgentStatusBadge.vue'
import { useChatSession } from '@/composables/workspace/useChatSession'
import { useChatStream, type StreamStep } from '@/composables/workspace/useChatStream'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useModelsStore } from '@/stores/modelsStore'
import { listAgents, getAgent, updateAgent } from '@/api/agents'
import { BackendError } from '@/api/http-client'
import { steerChatStream } from '@/api/chat-stream'
import {
  sendChatMessage as sendChatMessageApi,
  subscribeSessionEvents,
  type UnlistenFn,
} from '@/api/chat-session'
import { getExecutionThread, listExecutionSessions } from '@/api/execution-console'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import type { AgentFile, ModelOption } from '@/types/workspace'
import type { ModelId } from '@/types/generated/ModelId'
import type { VoiceMessageInfo } from '@/composables/workspace/useVoiceRecorder'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'
import type { ThreadSelection } from './threadItems'
import { buildSessionThreadItems, type ThreadItem } from './threadItems'
import {
  extractOperationAssessment,
  formatOperationAssessment,
  type OperationAssessment,
} from '@/utils/operationAssessment'
import { buildVoiceMessageContent } from './voiceMessageContent'

const props = withDefaults(
  defineProps<{
    selectedRunId?: string | null
    backgroundTaskId?: string | null
  }>(),
  {
    selectedRunId: null,
    backgroundTaskId: null,
  },
)

const emit = defineEmits<{
  showPanel: [resultJson: string]
  toolResult: [step: StreamStep]
  threadSelection: [selection: ThreadSelection]
  threadLoaded: [thread: ExecutionThread | null]
}>()

const toast = useToast()
const { confirm } = useConfirm()
const { t } = useI18n()
const router = useRouter()
const chatSessionStore = useChatSessionStore()
const backgroundAgentStore = useBackgroundAgentStore()
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
} = useChatSession({ autoLoad: true, autoSelectRecent: true })

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

// Stream state
const isStreaming = computed(() => chatStream.isStreaming.value)
const isExecuting = computed(() => isSending.value || isStreaming.value)
const streamContent = computed(() => chatStream.state.value.content)
const streamThinking = computed(() => chatStream.state.value.thinking)
const streamSteps = computed(() => chatStream.state.value.steps)
const threadItems = computed<ThreadItem[]>(() =>
  buildSessionThreadItems({
    thread: executionThread.value,
    messages: messages.value,
    steps: streamSteps.value,
    streamContent: streamContent.value,
  }),
)

// Token stats from stream
const inputTokens = computed(() => chatStream.state.value.inputTokens)
const outputTokens = computed(() => chatStream.state.value.outputTokens)
const totalTokens = computed(() => chatStream.state.value.tokenCount)
const tokensPerSecond = computed(() => chatStream.tokensPerSecond.value)
const durationMs = computed(() => chatStream.duration.value)

// Background agent linked to current session (if any)
const linkedBgAgent = computed(() => {
  if (props.backgroundTaskId) {
    return backgroundAgentStore.agents.find((agent) => agent.id === props.backgroundTaskId) ?? null
  }
  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return null
  return backgroundAgentStore.agentBySessionId(sessionId)
})
const isExternalSessionManaged = computed(() => {
  const source = currentSession.value?.source_channel
  return !!source && source !== 'workspace'
})
const bgCanPause = computed(() => linkedBgAgent.value?.status === 'active')
const bgCanResume = computed(() => linkedBgAgent.value?.status === 'paused')
const bgCanRun = computed(
  () => linkedBgAgent.value?.status === 'active' || linkedBgAgent.value?.status === 'paused',
)
const bgCanStop = computed(() => linkedBgAgent.value?.status === 'running')

async function handleBgPause() {
  if (!linkedBgAgent.value) return
  await backgroundAgentStore.pauseAgent(linkedBgAgent.value.id)
}

async function handleBgResume() {
  if (!linkedBgAgent.value) return
  await backgroundAgentStore.resumeAgent(linkedBgAgent.value.id)
}

async function handleBgRun() {
  if (!linkedBgAgent.value) return
  await backgroundAgentStore.runAgentNow(
    linkedBgAgent.value.id,
    async (assessment: OperationAssessment) =>
      confirm({
        title: 'Confirmation required',
        description: formatOperationAssessment(assessment),
        confirmText: 'Run anyway',
        cancelText: 'Cancel',
      }),
  )
}

async function handleBgStop() {
  if (!linkedBgAgent.value) return
  await backgroundAgentStore.stopAgent(linkedBgAgent.value.id)
}

async function handleOpenRunTrace() {
  const runId = props.selectedRunId || executionThread.value?.focus.run_id || null
  const containerId =
    executionThread.value?.focus.container_id ||
    props.backgroundTaskId ||
    linkedBgAgent.value?.id ||
    chatSessionStore.currentSessionId ||
    null
  if (runId) {
    await router.push({
      name: containerId ? 'workspace-container-run' : 'workspace-run-id',
      params: containerId ? { containerId, runId } : { runId },
    })
    return
  }

  if (!linkedBgAgent.value) return

  try {
    const runs = await listExecutionSessions({
      container: {
        kind: 'background_task',
        id: linkedBgAgent.value.id,
      },
    })
    const latestRunId = runs.find((entry) => !!entry.run_id)?.run_id ?? null
    const latestRunContainerId =
      runs.find((entry) => !!entry.run_id)?.container_id ?? linkedBgAgent.value.id
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
    params: { containerId: linkedBgAgent.value.id },
  })
}

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

async function loadExecutionThreadForSession(_sessionId: string | null) {
  const requestVersion = ++executionThreadLoadVersion

  if (!props.selectedRunId) {
    executionThread.value = null
    emit('threadLoaded', null)
    return
  }

  try {
    const thread = await getExecutionThread({
      session_id: null,
      run_id: props.selectedRunId,
      task_id: null,
    })

    if (requestVersion !== executionThreadLoadVersion) return
    executionThread.value = thread
    emit('threadLoaded', thread)
  } catch (error) {
    if (requestVersion !== executionThreadLoadVersion) return

    if (error instanceof BackendError && error.code === 404) {
      executionThread.value = null
      emit('threadLoaded', null)
      return
    }

    console.warn('Failed to load execution thread for session:', error)
    executionThread.value = null
    emit('threadLoaded', null)
  }
}

async function navigateToLatestWorkspaceRun(sessionId: string) {
  if (props.selectedRunId) return
  if (currentSession.value?.source_channel && currentSession.value.source_channel !== 'workspace') return

  try {
    const runs = await listExecutionSessions({
      container: {
        kind: 'workspace',
        id: sessionId,
      },
    })
    const latestRun = runs.find((entry) => !!entry.run_id)
    if (!latestRun?.run_id) return

    await router.replace({
      name: 'workspace-container-run',
      params: {
        containerId: latestRun.container_id || sessionId,
        runId: latestRun.run_id,
      },
    })
  } catch (error) {
    console.warn('Failed to resolve latest workspace run:', error)
  }
}

watch(
  () => [chatSessionStore.currentSessionId, props.selectedRunId],
  ([sessionId]) => {
    void loadExecutionThreadForSession(sessionId ?? null)
  },
  { immediate: true },
)

async function syncSessionFromBackend() {
  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return

  const refreshed = await chatSessionStore.refreshSession(sessionId)
  if (refreshed) {
    chatSessionStore.updateSessionLocally(refreshed)
    await loadExecutionThreadForSession(sessionId)
    await navigateToLatestWorkspaceRun(sessionId)
    // Clear stream content after persisted messages are loaded to prevent
    // showing both the streaming message and the persisted message.
    chatStream.reset()
  }
}

watch(isStreaming, (streaming, prevStreaming) => {
  if (prevStreaming && !streaming) {
    void syncSessionFromBackend()
  }
})

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
    await chatStream.send(message)
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
      try {
        await updateAgent(agentId, request)
      } catch (error) {
        const assessment = extractOperationAssessment(error)
        if (
          error instanceof BackendError &&
          error.code === 428 &&
          assessment?.confirmation_token
        ) {
          const confirmed = await confirm({
            title: 'Confirmation required',
            description: formatOperationAssessment(assessment),
            confirmText: 'Update anyway',
            cancelText: 'Cancel',
          })
          if (confirmed) {
            await updateAgent(agentId, {
              ...request,
              confirmation_token: assessment.confirmation_token,
            })
          }
        }
      }
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
  backgroundAgentStore.fetchAgents()

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
            return loadExecutionThreadForSession(sessionId).then(() =>
              navigateToLatestWorkspaceRun(sessionId),
            )
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
    <!-- Header: Background agent control bar or plain drag region -->
    <div
      v-if="linkedBgAgent"
      class="flex items-center gap-2 px-3 py-1.5 border-b border-border shrink-0 text-xs text-muted-foreground"
    >
      <AgentStatusBadge :status="linkedBgAgent.status" />
      <Button
        variant="ghost"
        size="sm"
        class="h-6 gap-1 px-2 text-xs"
        data-testid="open-run-trace"
        :title="t('backgroundAgent.openRunTrace')"
        @click="handleOpenRunTrace"
      >
        <Activity :size="12" />
        <span>{{ t('backgroundAgent.openRunTrace') }}</span>
      </Button>
      <div class="flex-1" />
      <Button
        v-if="bgCanPause"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.pause')"
        @click="handleBgPause"
      >
        <Pause :size="12" />
      </Button>
      <Button
        v-if="bgCanResume"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.resume')"
        @click="handleBgResume"
      >
        <RotateCcw :size="12" />
      </Button>
      <Button
        v-if="bgCanRun"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.runNow')"
        @click="handleBgRun"
      >
        <Play :size="12" />
      </Button>
      <Button
        v-if="bgCanStop"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.stop')"
        @click="handleBgStop"
      >
        <XCircle :size="12" />
      </Button>
    </div>
    <div v-else class="h-8 shrink-0" />

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
