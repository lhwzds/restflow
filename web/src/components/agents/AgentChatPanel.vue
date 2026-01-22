<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Send, Trash2, User, CircleCheck, Eye, EyeOff, Loader2 } from 'lucide-vue-next'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useAgentOperations } from '@/composables/agents/useAgentOperations'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import ExecutionStepDisplay from '@/components/agents/ExecutionStepDisplay.vue'
import { type ExecutionDetails, type ExecutionStep } from '@/api/agents'
import { getModelDisplayName } from '@/utils/AIModels'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'

const props = defineProps<{
  agent: StoredAgent
}>()

// Use inline execution to test with current (possibly unsaved) configuration
const { executeAgentInline } = useAgentOperations()

// Chat message type with execution details
interface Message {
  role: 'user' | 'assistant'
  content: string
  timestamp: Date
  error?: boolean
  executionDetails?: ExecutionDetails
  showDetails?: boolean // Toggle for execution details visibility
}

// Role icons mapping for avatar display
const roleIcons = {
  user: User,
  assistant: CircleCheck,
} as const

const messages = ref<Message[]>([])
const input = ref('')
const isLoading = ref(false)
const messagesContainer = ref<HTMLElement>()

function addMessage(
  role: 'user' | 'assistant',
  content: string,
  error = false,
  executionDetails?: ExecutionDetails,
) {
  messages.value.push({
    role,
    content,
    timestamp: new Date(),
    error,
    executionDetails,
    showDetails: false,
  })
  // Auto-scroll handled by CSS scroll-behavior: smooth
  setTimeout(() => scrollToBottom(), 0)
}

function toggleDetails(index: number) {
  const message = messages.value[index]
  if (message) {
    message.showDetails = !message.showDetails
  }
}

// Filter execution steps for display (exclude system and user messages)
function getDisplaySteps(details: ExecutionDetails): ExecutionStep[] {
  return details.steps.filter((step) => step.step_type !== 'system' && step.step_type !== 'user')
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
    // Use inline execution with current agent config (supports unsaved changes)
    const result = await executeAgentInline(props.agent.agent, userInput)
    addMessage('assistant', result.response, false, result.execution_details ?? undefined)
  } catch (err: unknown) {
    const errorMessage = err instanceof Error ? err.message : 'Execution failed, please try again'
    addMessage('assistant', errorMessage, true)
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
  // Show welcome message when chat panel loads
  addMessage('assistant', `Hello! I'm ${props.agent.name}. How can I help you?`)
})
</script>

<template>
  <div class="agent-chat-panel">
    <div class="chat-header">
      <div class="header-info">
        <h3>{{ agent.name }}</h3>
        <span class="model-tag">{{ getModelDisplayName(agent.agent.model) }}</span>
      </div>
      <Button v-if="messages.length > 1" variant="ghost" @click="handleClear">
        <Trash2 class="mr-2 h-4 w-4" />
        Clear Chat
      </Button>
    </div>

    <div ref="messagesContainer" class="messages-container">
      <div v-for="(message, index) in messages" :key="index" :class="['message', message.role]">
        <div class="message-avatar">
          <component :is="roleIcons[message.role]" :size="16" />
        </div>

        <div class="message-content">
          <div class="message-header">
            <span class="message-role">
              {{ message.role === 'user' ? 'You' : agent.name }}
            </span>
            <span class="message-time">
              {{ formatTime(message.timestamp) }}
            </span>
            <!-- Execution details toggle -->
            <template v-if="message.executionDetails">
              <Badge variant="info" class="details-tag">
                {{ message.executionDetails.iterations }} iterations
              </Badge>
              <Badge variant="warning" class="details-tag">
                {{ message.executionDetails.total_tokens }} tokens
              </Badge>
              <Button
                variant="ghost"
                size="sm"
                @click="toggleDetails(index)"
              >
                <component :is="message.showDetails ? EyeOff : Eye" class="mr-1 h-4 w-4" />
                {{ message.showDetails ? 'Hide Details' : 'Show Details' }}
              </Button>
            </template>
          </div>
          <div :class="['message-text', { error: message.error }]">
            <MarkdownRenderer v-if="!message.error" :content="message.content" />
            <template v-else>
              {{ message.content }}
            </template>
          </div>

          <!-- Execution details panel -->
          <div
            v-if="message.executionDetails && message.showDetails"
            class="execution-details-panel"
          >
            <div class="details-header">
              <span>Execution Steps</span>
              <Badge
                :variant="message.executionDetails.status === 'completed' ? 'success' : 'destructive'"
              >
                {{ message.executionDetails.status }}
              </Badge>
            </div>
            <div class="steps-list">
              <ExecutionStepDisplay
                v-for="(step, stepIndex) in getDisplaySteps(message.executionDetails)"
                :key="stepIndex"
                :step="step"
              />
            </div>
          </div>
        </div>
      </div>

      <div v-if="isLoading" class="message assistant loading">
        <div class="message-avatar">
          <CircleCheck :size="16" />
        </div>
        <div class="message-content">
          <Skeleton class="h-4 w-full mb-2" />
          <Skeleton class="h-4 w-3/4" />
        </div>
      </div>
    </div>

    <div class="input-area">
      <Textarea
        v-model="input"
        placeholder="Type a message... (Ctrl+Enter to send)"
        class="chat-textarea"
        :disabled="isLoading"
        @keydown="handleKeydown"
      />
      <Button
        :disabled="!input.trim() || isLoading"
        @click="handleSend"
        class="send-button"
      >
        <Loader2 v-if="isLoading" class="mr-2 h-4 w-4 animate-spin" />
        <Send v-else class="mr-2 h-4 w-4" />
        {{ isLoading ? 'Sending...' : 'Send' }}
      </Button>
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

        .details-tag {
          margin-left: var(--rf-spacing-sm);
        }

        .execution-details-panel {
          margin-top: var(--rf-spacing-md);
          padding: var(--rf-spacing-md);
          background: var(--rf-color-bg-secondary);
          border-radius: var(--rf-radius-base);
          border: 1px solid var(--rf-color-border-lighter);

          .details-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: var(--rf-spacing-md);
            padding-bottom: var(--rf-spacing-sm);
            border-bottom: 1px solid var(--rf-color-border-lighter);
            font-weight: var(--rf-font-weight-semibold);
            color: var(--rf-color-text-primary);
          }

          .steps-list {
            max-height: 400px;
            overflow-y: auto;
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
    align-items: flex-end;

    .chat-textarea {
      flex: 1;
      min-height: 60px;
      max-height: 120px;
      resize: none;
    }

    .send-button {
      flex-shrink: 0;
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
