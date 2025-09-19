<script setup lang="ts">
import { ref, nextTick, onMounted } from 'vue'
import { ElInput, ElButton, ElSkeleton } from 'element-plus'
import { Promotion, Delete, User, CircleCheck } from '@element-plus/icons-vue'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useAgentOperations } from '@/composables/agents/useAgentOperations'

const props = defineProps<{
  agent: StoredAgent
}>()

const { executeAgent } = useAgentOperations()

// Chat message type
interface Message {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: Date
  error?: boolean
}

// State
const messages = ref<Message[]>([])
const input = ref('')
const isLoading = ref(false)
const messagesContainer = ref<HTMLElement>()

// Generate message ID
let messageIdCounter = 0
function generateMessageId() {
  return `msg-${Date.now()}-${++messageIdCounter}`
}

// Add message
function addMessage(role: 'user' | 'assistant', content: string, error = false): Message {
  const message: Message = {
    id: generateMessageId(),
    role,
    content,
    timestamp: new Date(),
    error
  }
  messages.value.push(message)
  nextTick(() => {
    scrollToBottom()
  })
  return message
}

// Scroll to bottom
function scrollToBottom() {
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}

// Send message
async function handleSend() {
  if (!input.value.trim() || isLoading.value) return

  const userInput = input.value.trim()
  input.value = ''

  // Add user message
  addMessage('user', userInput)

  isLoading.value = true

  try {
    // Execute Agent
    const response = await executeAgent(props.agent.id, userInput)
    addMessage('assistant', response)
  } catch (err: any) {
    addMessage('assistant', err.message || 'Execution failed, please try again', true)
  } finally {
    isLoading.value = false
  }
}

// Clear chat
function handleClear() {
  messages.value = []
}

// Support Ctrl+Enter to send
function handleKeydown(event: Event | KeyboardEvent) {
  if ('ctrlKey' in event && event.ctrlKey && 'key' in event && event.key === 'Enter') {
    event.preventDefault()
    handleSend()
  }
}

// Format time
function formatTime(date: Date): string {
  return date.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit'
  })
}

// Add welcome message when component mounts
onMounted(() => {
  addMessage('assistant', `Hello! I'm ${props.agent.name}. How can I help you?`)
})
</script>

<template>
  <div class="agent-chat-panel">
    <!-- Header -->
    <div class="chat-header">
      <div class="header-info">
        <h3>{{ agent.name }}</h3>
        <span class="model-tag">{{ agent.agent.model }}</span>
      </div>
      <ElButton
        v-if="messages.length > 1"
        :icon="Delete"
        text
        @click="handleClear"
      >
        Clear Chat
      </ElButton>
    </div>

    <!-- Messages -->
    <div ref="messagesContainer" class="messages-container">
      <div
        v-for="message in messages"
        :key="message.id"
        :class="['message', message.role]"
      >
        <div class="message-avatar">
          <ElIcon v-if="message.role === 'user'">
            <User />
          </ElIcon>
          <ElIcon v-else>
            <CircleCheck />
          </ElIcon>
        </div>

        <div class="message-content">
          <div class="message-header">
            <span class="message-role">
              {{ message.role === 'user' ? 'You' : agent.name }}
            </span>
            <span class="message-time">
              {{ formatTime(message.timestamp) }}
            </span>
          </div>
          <div
            :class="['message-text', { error: message.error }]"
          >
            {{ message.content }}
          </div>
        </div>
      </div>

      <!-- Loading indicator -->
      <div v-if="isLoading" class="message assistant loading">
        <div class="message-avatar">
          <ElIcon>
            <CircleCheck />
          </ElIcon>
        </div>
        <div class="message-content">
          <ElSkeleton :rows="2" animated />
        </div>
      </div>
    </div>

    <!-- Input area -->
    <div class="input-area">
      <ElInput
        v-model="input"
        type="textarea"
        placeholder="Type a message... (Ctrl+Enter to send)"
        :autosize="{ minRows: 2, maxRows: 4 }"
        :disabled="isLoading"
        @keydown="handleKeydown"
      />
      <ElButton
        type="primary"
        :icon="Promotion"
        :loading="isLoading"
        :disabled="!input.trim() || isLoading"
        @click="handleSend"
        class="send-button"
      >
        {{ isLoading ? 'Sending...' : 'Send' }}
      </ElButton>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.agent-chat-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--rf-color-bg-page);

  .chat-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    background: var(--rf-color-bg-container);
    border-bottom: 1px solid var(--rf-color-border-lighter);

    .header-info {
      display: flex;
      align-items: center;
      gap: 12px;

      h3 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--rf-color-text-primary);
      }

      .model-tag {
        font-size: 12px;
        padding: 2px 8px;
        background: var(--rf-color-primary-light);
        color: var(--rf-color-primary);
        border-radius: 4px;
      }
    }
  }

  .messages-container {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
    scroll-behavior: smooth;

    .message {
      display: flex;
      gap: 12px;
      margin-bottom: 20px;
      animation: fadeIn 0.3s ease;

      &.user {
        .message-avatar {
          background: var(--rf-color-primary);
        }
      }

      &.assistant {
        .message-avatar {
          background: var(--rf-color-success);
        }
      }

      &.loading {
        .message-content {
          flex: 1;
        }
      }

      .message-avatar {
        width: 36px;
        height: 36px;
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        color: white;
        flex-shrink: 0;
      }

      .message-content {
        flex: 1;

        .message-header {
          display: flex;
          align-items: center;
          gap: 8px;
          margin-bottom: 4px;

          .message-role {
            font-weight: 500;
            color: var(--rf-color-text-primary);
            font-size: 14px;
          }

          .message-time {
            font-size: 12px;
            color: var(--rf-color-text-secondary);
          }
        }

        .message-text {
          background: var(--rf-color-bg-container);
          padding: 12px 16px;
          border-radius: 8px;
          line-height: 1.6;
          white-space: pre-wrap;
          word-wrap: break-word;
          color: var(--rf-color-text-regular);

          &.error {
            background: var(--rf-color-danger-light);
            color: var(--rf-color-danger);
            border: 1px solid var(--rf-color-danger);
          }
        }
      }
    }
  }

  .input-area {
    padding: 16px 20px;
    background: var(--rf-color-bg-container);
    border-top: 1px solid var(--rf-color-border-lighter);
    display: flex;
    gap: 12px;
    align-items: stretch;

    :deep(.el-textarea) {
      flex: 1;
      display: flex;
    }

    :deep(.el-textarea__inner) {
      height: 100%;
    }

    .send-button {
      align-self: stretch;
      display: flex;
      align-items: center;
      justify-content: center;
    }
  }
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

// Dark mode adaptation
html.dark {
  .agent-chat-panel {
    .chat-header {
      background-color: var(--rf-color-bg-container);

      .model-tag {
        background: var(--rf-color-primary);
        color: white;
        opacity: 0.9;
      }
    }

    .messages-container {
      .message {
        .message-content {
          .message-text {
            background: var(--rf-color-bg-container);
          }
        }
      }
    }

    .input-area {
      background-color: var(--rf-color-bg-container);
    }
  }
}
</style>
