<script setup lang="ts">
import { ref, watch, nextTick, useTemplateRef, toRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { Send, Square, X, Cpu, Mic, Loader2, AudioLines, Type } from 'lucide-vue-next'
import { useVoiceRecorder, getVoiceModel } from '@/composables/workspace/useVoiceRecorder'
import type { VoiceMode } from '@/composables/workspace/useVoiceRecorder'
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

// Voice mode selection (persisted in component state)
const selectedVoiceMode = ref<VoiceMode>('voice-to-text')

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

const handleMicClick = () => {
  recorder.toggleRecording(selectedVoiceMode.value)
}

const setVoiceMode = (mode: 'voice-to-text' | 'voice-message') => {
  selectedVoiceMode.value = mode
}
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

      <!-- Bottom Row -->
      <div class="flex items-center gap-2 mt-2">
        <!-- Recording state: show recording info + cancel + stop -->
        <template v-if="recorder.state.value.isRecording">
          <div class="flex-1" />

          <!-- Recording indicator -->
          <div class="flex items-center gap-1.5 text-xs text-destructive">
            <span class="w-2 h-2 rounded-full bg-destructive animate-pulse" />
            {{ t('voice.recording') }} {{ recorder.state.value.duration }}s
          </div>

          <!-- Cancel button -->
          <Button
            size="sm"
            variant="ghost"
            class="h-8 px-3 text-xs"
            @click="recorder.cancelRecording()"
          >
            <X :size="14" class="mr-1" />
            {{ t('common.cancel') }}
          </Button>

          <!-- Voice-message mode: Send button -->
          <Button
            v-if="selectedVoiceMode === 'voice-message'"
            size="sm"
            class="h-8 px-3 text-xs"
            @click="recorder.stopRecording()"
          >
            <Send :size="14" class="mr-1" />
            {{ t('common.send') }}
          </Button>

          <!-- Voice-to-text mode: Done button -->
          <Button
            v-else
            size="sm"
            variant="secondary"
            class="h-8 px-3 text-xs"
            @click="recorder.stopRecording()"
          >
            <Square :size="14" class="mr-1" />
            {{ t('voice.done') }}
          </Button>
        </template>

        <!-- Idle / Transcribing state: show normal controls -->
        <template v-else>
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

          <!-- Voice: mode selector + mic button -->
          <div
            v-if="!isExecuting && recorder.isSupported.value"
            class="flex items-center rounded-md border border-border h-8"
          >
            <!-- Voice-to-text mode -->
            <button
              type="button"
              :class="cn(
                'flex items-center gap-1 px-2 h-full text-xs rounded-l-md transition-colors',
                selectedVoiceMode === 'voice-to-text'
                  ? 'bg-primary/15 text-primary font-medium'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted/50',
              )"
              :disabled="recorder.state.value.isTranscribing"
              :title="t('voice.voiceToText')"
              @click="setVoiceMode('voice-to-text')"
            >
              <Type :size="13" />
              <span class="hidden sm:inline">{{ t('voice.voiceToText') }}</span>
            </button>

            <!-- Divider -->
            <div class="w-px h-4 bg-border" />

            <!-- Voice-message mode -->
            <button
              type="button"
              :class="cn(
                'flex items-center gap-1 px-2 h-full text-xs transition-colors',
                selectedVoiceMode === 'voice-message'
                  ? 'bg-primary/15 text-primary font-medium'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted/50',
              )"
              :disabled="recorder.state.value.isTranscribing"
              :title="t('voice.sendVoice')"
              @click="setVoiceMode('voice-message')"
            >
              <AudioLines :size="13" />
              <span class="hidden sm:inline">{{ t('voice.sendVoice') }}</span>
            </button>

            <!-- Divider -->
            <div class="w-px h-4 bg-border" />

            <!-- Mic button: start recording in selected mode -->
            <button
              type="button"
              class="flex items-center justify-center px-2 h-full rounded-r-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              :disabled="recorder.state.value.isTranscribing"
              @click="handleMicClick"
            >
              <Loader2 v-if="recorder.state.value.isTranscribing" :size="14" class="animate-spin" />
              <Mic v-else :size="14" />
            </button>
          </div>

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
        </template>
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
