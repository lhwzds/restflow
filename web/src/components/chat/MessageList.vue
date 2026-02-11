<script setup lang="ts">
/**
 * MessageList Component
 *
 * Renders chat messages with streaming support, tool call display,
 * and auto-scroll behavior.
 */
import { ref, watch, nextTick, onMounted } from 'vue'
import {
  Wrench,
  ChevronDown,
  ChevronRight,
  Check,
  X,
  Loader2,
  PanelRight,
  MessageSquarePlus,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import StreamingMarkdown from '@/components/shared/StreamingMarkdown.vue'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const props = defineProps<{
  messages: ChatMessage[]
  isStreaming: boolean
  streamContent: string
  streamThinking: string
  steps: StreamStep[]
}>()

const emit = defineEmits<{
  viewToolResult: [step: StreamStep]
}>()

const scrollContainer = ref<HTMLElement | null>(null)
const expandedToolCalls = ref<Set<number>>(new Set())

function toggleToolCall(index: number) {
  if (expandedToolCalls.value.has(index)) {
    expandedToolCalls.value.delete(index)
  } else {
    expandedToolCalls.value.add(index)
  }
}

function canViewStep(step: StreamStep): boolean {
  return step.type === 'tool_call' && step.status === 'completed' && !!step.result
}

function scrollToBottom() {
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight
  }
}

// Auto-scroll when new messages arrive or streaming content updates
watch(
  () => [props.messages.length, props.streamContent],
  async () => {
    await nextTick()
    scrollToBottom()
  },
)

onMounted(() => {
  nextTick(() => scrollToBottom())
})
</script>

<template>
  <div ref="scrollContainer" class="flex-1 overflow-auto px-4 py-4">
    <div class="max-w-[48rem] mx-auto space-y-4">
      <!-- Saved Messages -->
      <div
        v-for="(msg, idx) in messages"
        :key="msg.id || idx"
        :class="[
          'p-4 rounded-lg',
          msg.role === 'user'
            ? 'bg-primary/10 ml-auto max-w-[80%]'
            : 'bg-muted mr-auto max-w-[90%]',
        ]"
      >
        <div class="text-xs text-muted-foreground mb-1">
          {{ msg.role === 'user' ? 'You' : msg.role === 'assistant' ? 'Assistant' : 'System' }}
        </div>
        <StreamingMarkdown :content="msg.content || ''" />
      </div>

      <!-- Streaming Response (in-progress) -->
      <div
        v-if="isStreaming || streamContent"
        class="bg-muted mr-auto max-w-[90%] p-4 rounded-lg"
      >
        <div class="text-xs text-muted-foreground mb-1">Assistant</div>

        <!-- Thinking indicator -->
        <div
          v-if="streamThinking && !streamContent"
          class="text-sm text-muted-foreground italic mb-2"
        >
          Thinking...
        </div>

        <!-- Streaming content -->
        <StreamingMarkdown
          v-if="streamContent"
          :content="streamContent"
          :is-streaming="isStreaming"
        />

        <!-- Tool call steps -->
        <div v-if="steps.length > 0" class="mt-3 space-y-1">
          <div
            v-for="(step, idx) in steps"
            :key="idx"
            class="text-xs border border-border rounded-md overflow-hidden"
          >
            <!-- Tool call header -->
            <button
              class="w-full flex items-center gap-2 px-2 py-1.5 hover:bg-muted/50 transition-colors"
              @click="toggleToolCall(idx)"
            >
              <!-- Status icon -->
              <Loader2
                v-if="step.status === 'running'"
                :size="12"
                class="animate-spin text-primary shrink-0"
              />
              <Check
                v-else-if="step.status === 'completed'"
                :size="12"
                class="text-green-500 shrink-0"
              />
              <X
                v-else-if="step.status === 'failed'"
                :size="12"
                class="text-red-500 shrink-0"
              />

              <Wrench :size="12" class="text-muted-foreground shrink-0" />
              <span class="font-mono truncate flex-1 text-left">{{ step.name }}</span>

              <!-- View button for completed tool results -->
              <Button
                v-if="canViewStep(step)"
                variant="ghost"
                size="sm"
                class="h-5 px-1.5 text-[10px] gap-1"
                @click.stop="emit('viewToolResult', step)"
              >
                <PanelRight :size="10" />
                View
              </Button>

              <!-- Expand chevron -->
              <ChevronDown
                v-if="expandedToolCalls.has(idx)"
                :size="12"
                class="text-muted-foreground shrink-0"
              />
              <ChevronRight
                v-else
                :size="12"
                class="text-muted-foreground shrink-0"
              />
            </button>

            <!-- Expanded result -->
            <div
              v-if="expandedToolCalls.has(idx) && step.result"
              class="px-2 py-1.5 border-t border-border bg-muted/30"
            >
              <pre class="text-[11px] font-mono whitespace-pre-wrap break-words max-h-48 overflow-auto">{{ step.result }}</pre>
            </div>
          </div>
        </div>
      </div>

      <!-- Typing indicator -->
      <div
        v-if="isStreaming && !streamContent && !streamThinking"
        class="flex items-center gap-2 text-muted-foreground p-2"
      >
        <div
          class="animate-spin h-4 w-4 border-2 border-primary border-t-transparent rounded-full"
        />
        <span class="text-sm">Processing...</span>
      </div>

      <!-- Empty state -->
      <div
        v-if="messages.length === 0 && !isStreaming && !streamContent"
        class="flex flex-col items-center justify-center py-20 text-muted-foreground"
      >
        <MessageSquarePlus :size="32" class="mb-3 opacity-50" />
        <p class="text-sm">Start a new conversation</p>
      </div>
    </div>
  </div>
</template>
