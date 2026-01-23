<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { Send, X, Loader2, Bot, Cpu } from 'lucide-vue-next'
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
  close: []
  'update:selectedAgent': [value: string | null]
  'update:selectedModel': [value: string]
}>()

const inputMessage = ref('')

const handleSend = () => {
  const message = inputMessage.value.trim()
  if (message && !props.isExecuting) {
    emit('send', message)
    inputMessage.value = ''
  }
}

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Enter' && !e.shiftKey) {
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
        @click="emit('close')"
      >
        <X :size="16" />
      </Button>
    </div>

    <!-- Selectors Row -->
    <div class="flex items-center gap-3 mb-2">
      <!-- Agent Selector -->
      <div class="flex items-center gap-2">
        <Bot :size="16" class="text-muted-foreground" />
        <Select
          :model-value="selectedAgent || ''"
          @update:model-value="emit('update:selectedAgent', $event || null)"
        >
          <SelectTrigger class="w-[160px] h-8 text-sm">
            <SelectValue placeholder="Select Agent" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem
              v-for="agent in availableAgents"
              :key="agent.id"
              :value="agent.id"
            >
              {{ agent.name }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <!-- Model Selector -->
      <div class="flex items-center gap-2">
        <Cpu :size="16" class="text-muted-foreground" />
        <Select
          :model-value="selectedModel"
          @update:model-value="emit('update:selectedModel', $event)"
        >
          <SelectTrigger class="w-[180px] h-8 text-sm">
            <SelectValue placeholder="Select Model" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem
              v-for="model in availableModels"
              :key="model.id"
              :value="model.id"
            >
              {{ model.name }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
    </div>

    <!-- Input Area -->
    <div
      :class="cn(
        'flex items-end gap-2 bg-background border rounded-xl p-2 shadow-md',
        isExpanded ? 'border-primary/50' : 'border-border'
      )"
    >
      <Textarea
        v-model="inputMessage"
        placeholder="Ask the agent to do something..."
        class="chat-textarea flex-1 min-h-[40px] max-h-[120px] resize-none border-0 bg-transparent p-2 text-sm focus-visible:ring-0 focus-visible:ring-offset-0"
        :disabled="isExecuting"
        @keydown="handleKeydown"
        @input="handleInput"
      />

      <Button
        size="icon"
        class="h-9 w-9 shrink-0"
        :disabled="!inputMessage.trim() || isExecuting"
        @click="handleSend"
      >
        <Loader2 v-if="isExecuting" :size="16" class="animate-spin" />
        <Send v-else :size="16" />
      </Button>
    </div>

    <!-- Hints -->
    <div
      v-if="!isExpanded"
      class="mt-2 flex items-center justify-center gap-4 text-xs text-muted-foreground"
    >
      <span>Press <kbd class="px-1 py-0.5 bg-muted rounded text-[10px]">Enter</kbd> to send</span>
      <span><kbd class="px-1 py-0.5 bg-muted rounded text-[10px]">Shift+Enter</kbd> for new line</span>
    </div>
  </div>
</template>
