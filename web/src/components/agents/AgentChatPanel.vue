<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElInput, ElButton, ElSkeleton } from 'element-plus'
import { Promotion, Delete, User, CircleCheck } from '@element-plus/icons-vue'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useAgentOperations } from '@/composables/agents/useAgentOperations'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'

const props = defineProps<{
  agent: StoredAgent
}>()

const { executeAgent } = useAgentOperations()

// Chat message type (simplified - no ID needed)
interface Message {
  role: 'user' | 'assistant'
  content: string
  timestamp: Date
  error?: boolean
}

const roleIcons = {
  user: User,
  assistant: CircleCheck,
} as const

const messages = ref<Message[]>([])
const input = ref('')
const isLoading = ref(false)
const messagesContainer = ref<HTMLElement>()

function addMessage(role: 'user' | 'assistant', content: string, error = false) {
  messages.value.push({
    role,
    content,
    timestamp: new Date(),
    error,
  })
  // Auto-scroll handled by CSS scroll-behavior: smooth
  setTimeout(() => scrollToBottom(), 0)
}

// Scroll to bottom (simplified - let CSS handle smoothness)
function scrollToBottom() {
  messagesContainer.value?.scrollTo(0, messagesContainer.value.scrollHeight)
}

async function handleSend() {
  if (!input.value.trim() || isLoading.value) return

  const userInput = input.value.trim()
  input.value = ''

  addMessage('user', userInput)

  isLoading.value = true

  try {
    const response = await executeAgent(props.agent.id, userInput)
    addMessage('assistant', response)
  } catch (err: any) {
    addMessage('assistant', err.message || 'Execution failed, please try again', true)
  } finally {
    isLoading.value = false
  }
}

function handleClear() {
  messages.value = []
}

// Support Ctrl+Enter / Cmd+Enter to send
function handleKeydown(event: KeyboardEvent | Event) {
  if (event instanceof KeyboardEvent && (event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    handleSend()
  }
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
  })
}

onMounted(() => {
  addMessage('assistant', `Hello! I'm ${props.agent.name}. How can I help you?`)
})
</script>

<template>
  <div class="agent-chat-panel">
    <div class="chat-header">
      <div class="header-info">
        <h3>{{ agent.name }}</h3>
        <span class="model-tag">{{ agent.agent.model }}</span>
      </div>
      <ElButton v-if="messages.length > 1" :icon="Delete" text @click="handleClear">
        Clear Chat
      </ElButton>
    </div>

    <div ref="messagesContainer" class="messages-container">
      <div v-for="(message, index) in messages" :key="index" :class="['message', message.role]">
        <div class="message-avatar">
          <ElIcon>
            <component :is="roleIcons[message.role]" />
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
          <div :class="['message-text', { error: message.error }]">
            <MarkdownRenderer v-if="!message.error" :content="message.content" />
            <template v-else>
              {{ message.content }}
            </template>
          </div>
        </div>
      </div>

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
    padding: var(--rf-spacing-lg) var(--rf-spacing-xl);
    background: var(--rf-color-bg-container);
    border-bottom: 1px solid var(--rf-color-border-lighter);

    .header-info {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-md);

      h3 {
        margin: 0;
        font-size: var(--rf-font-size-md);
        font-weight: var(--rf-font-weight-semibold);
        color: var(--rf-color-text-primary);
      }

      .model-tag {
        font-size: var(--rf-font-size-xs);
        padding: var(--rf-spacing-3xs) var(--rf-spacing-sm);
        background: var(--rf-color-primary-bg-light);
        color: var(--rf-color-primary);
        border-radius: var(--rf-radius-small);
      }
    }
  }

  .messages-container {
    flex: 1;
    overflow-y: auto;
    padding: var(--rf-spacing-xl);
    scroll-behavior: smooth;

    .message {
      display: flex;
      gap: var(--rf-spacing-md);
      margin-bottom: var(--rf-spacing-xl);
      animation: fadeIn 0.2s ease-out;

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
        width: var(--rf-size-xs);
        height: var(--rf-size-xs);
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--rf-color-white);
        flex-shrink: 0;
      }

      .message-content {
        flex: 1;

        .message-header {
          display: flex;
          align-items: center;
          gap: var(--rf-spacing-sm);
          margin-bottom: var(--rf-spacing-xs);

          .message-role {
            font-weight: 500;
            color: var(--rf-color-text-primary);
            font-size: var(--rf-font-size-base);
          }

          .message-time {
            font-size: var(--rf-font-size-xs);
            color: var(--rf-color-text-secondary);
          }
        }

        .message-text {
          background: var(--rf-color-bg-container);
          padding: var(--rf-spacing-md) var(--rf-spacing-lg);
          border-radius: var(--rf-radius-large);
          line-height: 1.6;
          white-space: pre-wrap;
          word-wrap: break-word;
          color: var(--rf-color-text-regular);

          &.error {
            background: var(--rf-color-danger-light);
            color: var(--rf-color-danger);
            border: 1px solid var(--rf-color-danger);
          }

          // Reset margins and white-space for markdown content
          :deep(.markdown-renderer) {
            white-space: normal; // Override pre-wrap for markdown

            > *:first-child {
              margin-top: 0;
            }
            > *:last-child {
              margin-bottom: 0;
            }

            // Keep pre-wrap for code blocks
            pre {
              white-space: pre;
            }
          }
        }
      }
    }
  }

  .input-area {
    padding: var(--rf-spacing-lg) var(--rf-spacing-xl);
    background: var(--rf-color-bg-container);
    border-top: 1px solid var(--rf-color-border-lighter);
    display: flex;
    gap: var(--rf-spacing-md);
    align-items: stretch;

    :deep(.el-textarea) {
      flex: 1;
      display: flex;
    }

    :deep(.el-textarea__inner) {
      height: 100%;
    }

    .send-button {
      align-self: center;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: var(--rf-spacing-sm) var(--rf-spacing-md);
      margin-left: var(--rf-spacing-sm);
    }
  }
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

// Dark mode styles (minimal - CSS variables handle most)
html.dark {
  .model-tag {
    opacity: 0.9;
  }
}
</style>
