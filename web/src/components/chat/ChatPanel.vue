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
import { sendChatMessage as sendChatMessageApi } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type { AgentFile, ModelOption } from '@/types/workspace'
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

const {
  currentSession,
  messages: chatMessages,
  inputMessage,
  isSending,
  createSession: createChatSession,
  sendMessage: sendChatMessage,
} = useChatSession({ autoLoad: true, autoSelectRecent: true })

// Chat stream for real-time responses
const chatStream = useChatStream(() => chatSessionStore.currentSessionId)

const selectedAgent = ref<string | null>(null)
const selectedModel = ref('')
const availableAgents = ref<AgentFile[]>([])
const availableModels = ref<ModelOption[]>([])

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

// Sync agent/model from current session and reset stream on session change
watch(currentSession, (session) => {
  chatStream.reset()
  processedToolIds.value.clear()
  if (session) {
    selectedAgent.value = session.agent_id
    selectedModel.value = session.model
  }
})

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
    availableModels.value = modelsStore.getAllModels.map((model) => ({
      id: model.model,
      name: model.name,
    }))

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
    // Steering: save user message without resetting stream or triggering new execution
    const sessionId = chatSessionStore.currentSessionId
    if (sessionId) {
      try {
        const session = await sendChatMessageApi(sessionId, message)
        chatSessionStore.updateSessionLocally(session)
      } catch (error) {
        toast.error(error instanceof Error ? error.message : t('chat.sendMessageFailed'))
      }
    }
    return
  }

  // Normal send: reset stream and trigger execution
  chatStream.reset()
  processedToolIds.value.clear()

  lastUserContent.value = message
  inputMessage.value = message
  await sendChatMessage()

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

  chatStream.reset()
  processedToolIds.value.clear()

  inputMessage.value = lastUserContent.value
  await sendChatMessage()

  if (chatSessionStore.error) {
    toast.error(chatSessionStore.error)
  }
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
      @view-tool-result="onViewToolResult"
      @regenerate="handleRegenerate"
    />

    <!-- Input Area -->
    <div class="shrink-0 px-4 pb-4">
      <ChatBox
        :key="`chatbox-${availableAgents.length}-${availableModels.length}`"
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
        @cancel="handleCancel"
        @close="() => {}"
        @update:selected-agent="onUpdateSelectedAgent"
        @update:selected-model="onUpdateSelectedModel"
      />
    </div>
  </div>
</template>
