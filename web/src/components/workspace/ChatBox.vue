<script setup lang="ts">
import { ref, watch, nextTick, useTemplateRef, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { Send, Square, X, Cpu, Mic, Loader2 } from 'lucide-vue-next'
import { useVoiceRecorder, getVoiceModel } from '@/composables/workspace/useVoiceRecorder'
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
import { useToast } from '@/composables/useToast'
import SessionAgentSelector from '@/components/workspace/SessionAgentSelector.vue'
import TokenCounter from '@/components/chat/TokenCounter.vue'
import type { AgentFile, ModelOption } from '@/types/workspace'

const props = defineProps<{
  isExpanded: boolean
  isExecuting: boolean
  selectedAgent: string | null
  selectedModel: string
  availableAgents: AgentFile[]
  availableModels: ModelOption[]
  isStreaming?: boolean
  inputTokens?: number
  outputTokens?: number
  totalTokens?: number
  tokensPerSecond?: number
  durationMs?: number
}>()

const emit = defineEmits<{
  send: [message: string]
  cancel: []
  close: []
  sendVoiceMessage: [filePath: string]
  'update:selectedAgent': [value: string | null]
  'update:selectedModel': [value: string]
}>()

const { t } = useI18n()
const toast = useToast()
const inputMessage = ref('')
const textareaRef = useTemplateRef<InstanceType<typeof Textarea>>('chatTextarea')

// Voice recorder
const recorder = useVoiceRecorder({
  model: getVoiceModel(),
  onTranscribed: (text) => {
    if (text.trim()) {
      inputMessage.value += text
    }
  },
  onVoiceMessage: (filePath) => {
    emit('sendVoiceMessage', filePath)
  },
})

// Show toast on voice recorder errors
watch(
  () => recorder.state.value.error,
  (error) => {
    if (!error) return
    const message =
      error === 'mic_permission_denied'
        ? t('voice.micPermissionDenied')
        : error === 'mic_not_available'
          ? t('voice.micPermissionDenied')
          : t('voice.transcriptionFailed')
    toast.error(message)
  },
)

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
    const el = textareaRef.value?.$el as HTMLTextAreaElement | undefined
    if (el) {
      el.style.height = 'auto'
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
        :aria-label="t('chat.collapseInput')"
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
      <!-- Recording / Transcribing indicator -->
      <div
        v-if="recorder.state.value.isRecording || recorder.state.value.isTranscribing"
        class="flex items-center gap-2 text-xs mb-2"
        :class="recorder.state.value.isTranscribing ? 'text-muted-foreground' : 'text-destructive'"
      >
        <template v-if="recorder.state.value.isRecording">
          <span class="w-2 h-2 rounded-full bg-destructive animate-pulse" />
          {{ t('voice.recording') }} {{ recorder.state.value.duration }}s
          <span v-if="recorder.state.value.mode === 'voice-message'" class="text-muted-foreground">
            ({{ t('voice.voiceMessage') }})
          </span>
        </template>
        <template v-else>
          <Loader2 :size="12" class="animate-spin" />
          {{ t('voice.transcribing') }}
        </template>
      </div>

      <!-- Textarea -->
      <Textarea
        ref="chatTextarea"
        v-model="inputMessage"
        :placeholder="t('workspace.askAgent')"
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
            <SelectValue :placeholder="t('common.model')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="model in availableModels" :key="model.id" :value="model.id">
              {{ model.name }}
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Token Counter -->
        <TokenCounter
          v-if="totalTokens || isStreaming"
          :input-tokens="inputTokens"
          :output-tokens="outputTokens"
          :total-tokens="totalTokens"
          :tokens-per-second="tokensPerSecond"
          :duration-ms="durationMs"
          :is-streaming="isStreaming"
          compact
        />

        <!-- Spacer -->
        <div class="flex-1" />

        <!-- Mic Button -->
        <Button
          v-if="!isExecuting && recorder.isSupported.value"
          size="sm"
          variant="ghost"
          class="h-8 w-8 p-0"
          :class="{
            'text-destructive animate-pulse': recorder.state.value.isRecording,
          }"
          :disabled="recorder.state.value.isTranscribing"
          @mousedown.prevent="recorder.startRecording()"
          @mouseup.prevent="recorder.stopRecording()"
          @mouseleave="recorder.state.value.isRecording && recorder.stopRecording()"
        >
          <Mic v-if="!recorder.state.value.isTranscribing" :size="14" />
          <Loader2 v-else :size="14" class="animate-spin" />
        </Button>

        <!-- Stop Button (during execution) -->
        <Button
          v-if="isExecuting"
          size="sm"
          variant="destructive"
          class="h-8 px-4"
          @click="emit('cancel')"
        >
          <Square :size="14" class="mr-1" />
          {{ t('common.stop') }}
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
          {{ t('common.send') }}
        </Button>
      </div>
    </div>

    <!-- Hints -->
    <div
      v-if="!isExpanded"
      class="mt-2 flex items-center justify-center gap-4 text-xs text-muted-foreground"
    >
      <span>{{
        t('workspace.pressEnterToSend', {
          shortcut: 'Enter',
        })
      }}</span>
      <span>{{
        t('workspace.shiftEnterForNewLine', {
          shortcut: 'Shift+Enter',
        })
      }}</span>
    </div>
  </div>
</template>
