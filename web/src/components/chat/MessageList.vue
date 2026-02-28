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
  Copy,
  RefreshCw,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import StreamingMarkdown from '@/components/shared/StreamingMarkdown.vue'
import VoiceMessageBubble from '@/components/chat/VoiceMessageBubble.vue'
import { readMediaFile } from '@/api/voice'
import { useToast } from '@/composables/useToast'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const VOICE_MSG_PATTERN =
  /^\[Voice message[^\]]*\]\n\n\[Media Context\]\nmedia_type: voice\nlocal_file_path: (.+)\ninstruction:/

const props = defineProps<{
  messages: ChatMessage[]
  isStreaming: boolean
  streamContent: string
  streamThinking: string
  steps: StreamStep[]
  voiceAudioUrls?: Map<string, { blobUrl: string; duration: number }>
}>()

const emit = defineEmits<{
  viewToolResult: [step: StreamStep]
  regenerate: []
}>()

const toast = useToast()

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
  return (
    step.type === 'tool_call' &&
    (step.status === 'completed' || step.status === 'failed') &&
    !!step.result
  )
}

function isLastAssistantMessage(idx: number): boolean {
  for (let i = props.messages.length - 1; i >= 0; i--) {
    if (props.messages[i]?.role === 'assistant') {
      return i === idx
    }
  }
  return false
}

async function copyMessage(content: string) {
  try {
    await navigator.clipboard.writeText(content)
    toast.success('Copied to clipboard')
  } catch {
    toast.error('Failed to copy')
  }
}

/** Cache for blob URLs loaded from persistent storage */
const loadedMediaUrls = ref<Map<string, { blobUrl: string; duration: number }>>(new Map())
/** Tracks file paths currently being loaded to avoid duplicate requests */
const loadingMediaPaths = ref<Set<string>>(new Set())

function getVoiceFilePath(msg: ChatMessage): string | null {
  if (msg.role !== 'user') return null
  const match = msg.content.match(VOICE_MSG_PATTERN)
  return match?.[1] ?? null
}

function getVoiceAudio(msg: ChatMessage): { blobUrl: string; duration: number } | null {
  const filePath = getVoiceFilePath(msg)
  if (!filePath) return null
  // Check in-memory cache first (fresh recordings from this session)
  const cached = props.voiceAudioUrls?.get(filePath)
  if (cached) return cached
  // Check persistent storage cache (loaded from disk after page reload)
  const loaded = loadedMediaUrls.value.get(filePath)
  if (loaded) return loaded
  // Trigger async load if not already loading
  if (!loadingMediaPaths.value.has(filePath)) {
    loadingMediaPaths.value.add(filePath)
    loadMediaFromDisk(filePath)
  }
  return null
}

async function loadMediaFromDisk(filePath: string) {
  try {
    const base64 = await readMediaFile(filePath)
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i)
    }
    const ext = filePath.split('.').pop()?.toLowerCase() ?? 'webm'
    const mimeType = ext === 'ogg' || ext === 'oga' ? 'audio/ogg' : `audio/${ext}`
    const blob = new Blob([bytes], { type: mimeType })
    const blobUrl = URL.createObjectURL(blob)
    loadedMediaUrls.value.set(filePath, { blobUrl, duration: 0 })
  } catch {
    // File not found or not readable — silently ignore
  } finally {
    loadingMediaPaths.value.delete(filePath)
  }
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
      <div v-for="(msg, idx) in messages" :key="msg.id || idx" class="group relative">
        <div
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
          <!-- Voice message: show audio player or loading state -->
          <VoiceMessageBubble
            v-if="getVoiceAudio(msg)"
            :blob-url="getVoiceAudio(msg)!.blobUrl"
            :duration="getVoiceAudio(msg)!.duration"
          />
          <div
            v-else-if="getVoiceFilePath(msg) && loadingMediaPaths.has(getVoiceFilePath(msg)!)"
            class="flex items-center gap-2 text-xs text-muted-foreground py-1"
          >
            <Loader2 :size="12" class="animate-spin" />
            Loading voice message...
          </div>
          <!-- Regular message -->
          <StreamingMarkdown v-else :content="msg.content || ''" />
        </div>
        <!-- Hover action buttons -->
        <div
          :class="[
            'absolute -bottom-2 opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 z-10',
            msg.role === 'user' ? 'right-2' : 'left-2',
          ]"
        >
          <Button
            v-if="msg.content"
            variant="outline"
            size="sm"
            class="h-6 px-2 text-[10px] bg-background"
            @click="copyMessage(msg.content)"
          >
            <Copy :size="10" class="mr-1" />
            Copy
          </Button>
          <Button
            v-if="isLastAssistantMessage(idx) && !isStreaming"
            variant="outline"
            size="sm"
            class="h-6 px-2 text-[10px] bg-background"
            @click="emit('regenerate')"
          >
            <RefreshCw :size="10" class="mr-1" />
            Retry
          </Button>
        </div>
      </div>

      <!-- Streaming Response (in-progress) -->
      <div v-if="isStreaming || streamContent" class="bg-muted mr-auto max-w-[90%] p-4 rounded-lg">
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
              <X v-else-if="step.status === 'failed'" :size="12" class="text-red-500 shrink-0" />

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
              <ChevronRight v-else :size="12" class="text-muted-foreground shrink-0" />
            </button>

            <!-- Expanded result -->
            <div
              v-if="expandedToolCalls.has(idx) && step.result"
              class="px-2 py-1.5 border-t border-border bg-muted/30"
            >
              <pre
                class="text-[11px] font-mono whitespace-pre-wrap break-words max-h-48 overflow-auto"
                >{{ step.result }}</pre
              >
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
