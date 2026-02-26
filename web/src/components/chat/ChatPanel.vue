<script setup lang="ts">
/**
 * ChatPanel Component
 *
 * Main chat area combining header, message list, and input.
 * Integrates with useChatStream for real-time streaming and
 * detects show_panel tool calls for Canvas panel display.
 */
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import MessageList from './MessageList.vue'
import ChatBox from '@/components/workspace/ChatBox.vue'
import { useChatSession } from '@/composables/workspace/useChatSession'
import { useChatStream, type StreamStep } from '@/composables/workspace/useChatStream'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useModelsStore } from '@/stores/modelsStore'
import { listAgents } from '@/api/agents'
import { steerChatStream } from '@/api/chat-stream'
import { sendChatMessage as sendChatMessageApi } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type { AgentFile, ModelOption } from '@/types/workspace'
import type { VoiceMessageInfo } from '@/composables/workspace/useVoiceRecorder'
import type { ChatMessage } from '@/types/generated/ChatMessage'

const emit = defineEmits<{
  showPanel: [resultJson: string]
  toolResult: [step: StreamStep]
}>()

const toast = useToast()
const { t } = useI18n()
const chatSessionStore = useChatSessionStore()
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
  })),
)
const chatBoxKey = computed(() => {
  const sessionId = currentSession.value?.id ?? 'new'
  return `chatbox-${sessionId}-${availableAgents.value.length}-${availableModels.value.length}`
})

// Messages from store
const messages = computed<ChatMessage[]>(() => chatMessages.value)

// Stream state
const isStreaming = computed(() => chatStream.isStreaming.value)
const isExecuting = computed(() => isSending.value || isStreaming.value)
const streamContent = computed(() => chatStream.state.value.content)
const streamThinking = computed(() => chatStream.state.value.thinking)
const streamSteps = computed(() => chatStream.state.value.steps)

// Token stats from stream
const inputTokens = computed(() => chatStream.state.value.inputTokens)
const outputTokens = computed(() => chatStream.state.value.outputTokens)
const totalTokens = computed(() => chatStream.state.value.tokenCount)
const tokensPerSecond = computed(() => chatStream.tokensPerSecond.value)
const durationMs = computed(() => chatStream.duration.value)

// Track processed tool call IDs to avoid duplicate emits
const processedToolIds = ref<Set<string>>(new Set())

// Completed tool steps (computed avoids deep-watching entire steps array)
const completedToolSteps = computed(() =>
  chatStream.state.value.steps.filter(
    (s) => s.type === 'tool_call' && s.status === 'completed' && s.result && s.toolId,
  ),
)

// Watch only when new tool steps complete (by length change)
watch(
  () => completedToolSteps.value.length,
  () => {
    for (const step of completedToolSteps.value) {
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

async function syncSessionFromBackend() {
  const sessionId = chatSessionStore.currentSessionId
  if (!sessionId) return

  const refreshed = await chatSessionStore.refreshSession(sessionId)
  if (refreshed) {
    chatSessionStore.updateSessionLocally(refreshed)
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

  const updated = await chatSessionStore.updateSessionModel(session.id, model)
  if (!updated) {
    selectedModel.value = oldModel
    toast.error(t('chat.updateSessionModelFailed'))
    return
  }

  selectedModel.value = updated.model
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

async function onSendVoiceMessage(info: VoiceMessageInfo) {
  const message = `[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: ${info.filePath}\ninstruction: Use the transcribe tool with this file_path before answering.`

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

onMounted(() => {
  loadAgents()
  loadModels()
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
    <!-- Invisible drag region (replaces removed ChatHeader) -->
    <div class="h-8 shrink-0" data-tauri-drag-region />

    <!-- Message List -->
    <MessageList
      :messages="messages"
      :is-streaming="isStreaming"
      :stream-content="streamContent"
      :stream-thinking="streamThinking"
      :steps="streamSteps"
      :voice-audio-urls="voiceAudioUrls"
      @view-tool-result="onViewToolResult"
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
