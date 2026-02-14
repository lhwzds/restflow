<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { Send, Square, X, Cpu } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'
import SessionAgentSelector from '@/components/workspace/SessionAgentSelector.vue'
import type { AgentFile, ModelOption } from '@/types/workspace'

const props = defineProps<{
  isExpanded: boolean
  isExecuting: boolean
  selectedAgent: string | null
  selectedModel: string
  availableAgents: AgentFile[]
  availableModels: ModelOption[]
}>()

const emit = defineEmits<{
  send: [message: string]
  cancel: []
  close: []
  'update:selectedAgent': [value: string | null]
  'update:selectedModel': [value: string]
}>()

const inputMessage = ref('')

// Track IME composition state manually (WebKit's e.isComposing is unreliable)
const composing = ref(false)

// Delay clearing composing flag so keydown fires while still composing.
// WebKit fires compositionend BEFORE the Enter keydown, unlike Chrome.
const onCompositionEnd = () => {
  window.setTimeout(() => {
    composing.value = false
  }, 0)
}

const handleSend = () => {
  const message = inputMessage.value.trim()
  if (message) {
    emit('send', message)
    inputMessage.value = ''
  }
}

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Enter' && !e.shiftKey && !composing.value) {
    e.preventDefault()
    handleSend()
  }
}

// Auto-resize textarea on input
const handleInput = (e: Event) => {
  const textarea = e.target as HTMLTextAreaElement
  textarea.style.height = 'auto'
  textarea.style.height = `${Math.min(textarea.scrollHeight, 120)}px`
}

// Reset height when message is cleared
watch(inputMessage, async (newVal) => {
  if (!newVal) {
    await nextTick()
    // Find the textarea in the DOM and reset its height
    const textarea = document.querySelector('.chat-textarea') as HTMLTextAreaElement | null
    if (textarea) {
      textarea.style.height = 'auto'
    }
  }
})
</script>

<template>
  <div class="transition-all duration-300">
    <!-- Close button when expanded -->
    <div v-if="isExpanded" class="flex justify-end pb-2">
      <Button
        variant="ghost"
        size="icon"
        class="h-7 w-7"
        aria-label="Collapse chat input"
        @click="emit('close')"
      >
        <X :size="16" />
      </Button>
    </div>

    <!-- Input Area -->
    <div
      :class="
        cn(
          'flex flex-col bg-background border rounded-xl p-3',
          isExpanded ? 'border-primary/50' : 'border-border',
        )
      "
    >
      <!-- Textarea -->
      <Textarea
        v-model="inputMessage"
        placeholder="Ask the agent to do something..."
        class="chat-textarea min-h-[40px] max-h-[120px] resize-none border-0 bg-transparent p-0 text-sm shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
        @keydown="handleKeydown"
        @input="handleInput"
        @compositionstart="composing = true"
        @compositionend="onCompositionEnd"
      />

      <!-- Bottom Row: Agent | Model | Send -->
      <div class="flex items-center gap-2 mt-2">
        <!-- Agent Selector -->
        <SessionAgentSelector
          :selected-agent="selectedAgent"
          :available-agents="availableAgents"
          :disabled="isExecuting"
          @update:selected-agent="emit('update:selectedAgent', $event)"
        />

        <!-- Model Selector -->
        <Select
          :model-value="selectedModel"
          @update:model-value="emit('update:selectedModel', $event)"
        >
          <SelectTrigger class="w-[180px] h-8 text-xs">
            <Cpu :size="14" class="mr-1 text-muted-foreground shrink-0" />
            <SelectValue placeholder="Model" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="model in availableModels" :key="model.id" :value="model.id">
              {{ model.name }}
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Spacer -->
        <div class="flex-1" />

        <!-- Stop Button (during execution) -->
        <Button
          v-if="isExecuting"
          size="sm"
          variant="destructive"
          class="h-8 px-4"
          @click="emit('cancel')"
        >
          <Square :size="14" class="mr-1" />
          Stop
        </Button>

        <!-- Send Button -->
        <Button
          v-else
          size="sm"
          class="h-8 px-4"
          :disabled="!inputMessage.trim()"
          @click="handleSend"
        >
          <Send :size="14" class="mr-1" />
          Send
        </Button>
      </div>
    </div>

    <!-- Hints -->
    <div
      v-if="!isExpanded"
      class="mt-2 flex items-center justify-center gap-4 text-xs text-muted-foreground"
    >
      <span>Press <kbd class="px-1 py-0.5 bg-muted rounded text-[10px]">Enter</kbd> to send</span>
      <span
        ><kbd class="px-1 py-0.5 bg-muted rounded text-[10px]">Shift+Enter</kbd> for new line</span
      >
    </div>
  </div>
</template>
