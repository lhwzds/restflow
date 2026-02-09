<script setup lang="ts">
/**
 * ChatPanel Component
 *
 * Main chat area combining header, message list, and input.
 * Integrates with useChatStream for real-time streaming and
 * detects show_panel tool calls for Canvas panel display.
 */
import { ref, computed, watch, onMounted } from 'vue'
import ChatHeader from './ChatHeader.vue'
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
}>()

const toast = useToast()
const chatSessionStore = useChatSessionStore()
const modelsStore = useModelsStore()

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

// Current agent/model display names
const agentName = computed(() => {
  if (!selectedAgent.value) return undefined
  return availableAgents.value.find((a) => a.id === selectedAgent.value)?.name
})
const modelName = computed(() => {
  if (!selectedModel.value) return undefined
  return availableModels.value.find((m) => m.id === selectedModel.value)?.name
})

// Track processed show_panel steps to avoid duplicate emits
const processedShowPanelIds = ref<Set<string>>(new Set())

// Completed show_panel steps (computed avoids deep-watching entire steps array)
const completedShowPanelSteps = computed(() =>
  chatStream.state.value.steps.filter(
    (s) => s.name === 'show_panel' && s.status === 'completed' && s.result && s.toolId,
  ),
)

// Watch only when new show_panel steps complete (by length change)
watch(
  () => completedShowPanelSteps.value.length,
  () => {
    for (const step of completedShowPanelSteps.value) {
      if (!processedShowPanelIds.value.has(step.toolId!)) {
        processedShowPanelIds.value.add(step.toolId!)
        emit('showPanel', step.result!)
      }
    }
  },
)

// Sync agent/model from current session and reset stream on session change
watch(currentSession, (session) => {
  chatStream.reset()
  processedShowPanelIds.value.clear()
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
    const message = error instanceof Error ? error.message : 'Failed to load agents'
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
    const message = error instanceof Error ? error.message : 'Failed to load models'
    toast.error(message)
  }
}

async function ensureChatSession(): Promise<boolean> {
  if (chatSessionStore.currentSessionId) {
    return true
  }

  if (!selectedAgent.value) {
    toast.error('Select an agent to start a chat')
    return false
  }

  if (!selectedModel.value) {
    toast.error('Select a model to start a chat')
    return false
  }

  const session = await createChatSession(selectedAgent.value, selectedModel.value)
  if (!session) {
    toast.error('Failed to create chat session')
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
        toast.error(error instanceof Error ? error.message : 'Failed to send message')
      }
    }
    return
  }

  // Normal send: reset stream and trigger execution
  chatStream.reset()
  processedShowPanelIds.value.clear()

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
    toast.error('Failed to update session agent')
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
    toast.error('Failed to update session model')
    return
  }

  selectedModel.value = updated.model
}

function onViewInCanvas(step: StreamStep) {
  if (step.result) {
    emit('showPanel', step.result)
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
    <!-- Header (compact status bar) -->
    <ChatHeader
      :agent-name="agentName"
      :model-name="modelName"
      :is-streaming="isStreaming"
      :input-tokens="inputTokens"
      :output-tokens="outputTokens"
      :total-tokens="totalTokens"
      :tokens-per-second="tokensPerSecond"
      :duration-ms="durationMs"
    />

    <!-- Message List -->
    <MessageList
      :messages="messages"
      :is-streaming="isStreaming"
      :stream-content="streamContent"
      :stream-thinking="streamThinking"
      :steps="streamSteps"
      @view-in-canvas="onViewInCanvas"
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
        @send="onSendMessage"
        @close="() => {}"
        @update:selected-agent="onUpdateSelectedAgent"
        @update:selected-model="onUpdateSelectedModel"
      />
    </div>
  </div>
</template>
