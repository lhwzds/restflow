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
  Activity,
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
import type { ExecutionStepInfo } from '@/types/generated/ExecutionStepInfo'
import type { StepStatus } from '@/types/generated/StepStatus'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import { extractVoiceFilePath, extractVoiceTranscript } from './voiceMessageContent'

const props = withDefaults(
  defineProps<{
    messages: ChatMessage[]
    isStreaming: boolean
    streamContent: string
    streamThinking?: string
    steps?: StreamStep[]
    voiceAudioUrls?: Map<string, { blobUrl: string; duration: number }>
    enableCopyAction?: boolean
    enableRegenerateAction?: boolean
  }>(),
  {
    streamThinking: '',
    steps: () => [],
    enableCopyAction: true,
    enableRegenerateAction: true,
  },
)

const emit = defineEmits<{
  viewToolResult: [step: StreamStep]
  regenerate: []
}>()

const toast = useToast()

const scrollContainer = ref<HTMLElement | null>(null)
const expandedToolCalls = ref<Set<string>>(new Set())

function toggleToolCall(key: string) {
  if (expandedToolCalls.value.has(key)) {
    expandedToolCalls.value.delete(key)
  } else {
    expandedToolCalls.value.add(key)
  }
}

function canViewStep(step: StreamStep): boolean {
  return (step.status === 'completed' || step.status === 'failed') && !!step.result
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

function normalizeStepStatus(status: string): StepStatus {
  switch (status) {
    case 'completed':
    case 'failed':
    case 'pending':
    case 'running':
      return status
    default:
      return 'completed'
  }
}

function persistedStepKey(messageId: string, index: number): string {
  return `persisted:${messageId}:${index}`
}

function streamStepKey(index: number): string {
  return `stream:${index}`
}

function isToolStep(step: Pick<StreamStep, 'type'>): boolean {
  return step.type === 'tool_call'
}

function formatDurationLabel(durationMs: bigint | number | null | undefined): string | null {
  if (durationMs == null) return null
  return `${(Number(durationMs) / 1000).toFixed(1)}s`
}

function buildPersistedStep(messageId: string, step: ExecutionStepInfo, index: number): StreamStep {
  const toolId = `persisted-${messageId}-${index}`
  const metadata = {
    persisted_execution_step: true,
    message_id: messageId,
    step_index: index,
    step_type: step.step_type,
    name: step.name,
    status: step.status,
    duration_ms: step.duration_ms == null ? null : Number(step.duration_ms),
  }

  return {
    type: step.step_type === 'tool_call' ? 'tool_call' : step.step_type,
    name: step.name,
    displayName: step.name,
    status: normalizeStepStatus(step.status),
    toolId,
    arguments: JSON.stringify(metadata),
    result: JSON.stringify(
      {
        ...metadata,
        note:
          step.step_type === 'tool_call'
            ? 'Detailed persisted tool payload is not available yet.'
            : 'Persisted execution step summary.',
      },
      null,
      2,
    ),
  }
}

function persistedSteps(message: ChatMessage): StreamStep[] {
  if (message.role !== 'assistant' || !message.execution?.steps?.length) {
    return []
  }

  return message.execution.steps.map((step, index) => buildPersistedStep(message.id, step, index))
}

/** Cache for blob URLs loaded from persistent storage */
const loadedMediaUrls = ref<Map<string, { blobUrl: string; duration: number }>>(new Map())
/** Tracks file paths currently being loaded to avoid duplicate requests */
const loadingMediaPaths = ref<Set<string>>(new Set())

function getVoiceFilePath(msg: ChatMessage): string | null {
  if (msg.role !== 'user') return null
  if (msg.media?.media_type === 'voice') {
    const structuredPath = msg.media.file_path?.trim()
    if (structuredPath) return structuredPath
  }
  return extractVoiceFilePath(msg.content)
}

function getVoiceAudio(msg: ChatMessage): { blobUrl: string; duration: number } | null {
  const filePath = getVoiceFilePath(msg)
  if (!filePath) return null
  const structuredDuration = msg.media?.media_type === 'voice' ? (msg.media.duration_sec ?? 0) : 0
  // Check in-memory cache first (fresh recordings from this session)
  const cached = props.voiceAudioUrls?.get(filePath)
  if (cached) return { ...cached, duration: cached.duration || structuredDuration }
  // Check persistent storage cache (loaded from disk after page reload)
  const loaded = loadedMediaUrls.value.get(filePath)
  if (loaded) return { ...loaded, duration: loaded.duration || structuredDuration }
  // Trigger async load if not already loading
  if (!loadingMediaPaths.value.has(filePath)) {
    loadingMediaPaths.value.add(filePath)
    loadMediaFromDisk(filePath)
  }
  return null
}

function getVoiceTranscript(msg: ChatMessage): string | null {
  const filePath = getVoiceFilePath(msg)
  if (!filePath) return null
  const structuredTranscript = msg.transcript?.text?.trim()
  if (structuredTranscript) return structuredTranscript
  return extractVoiceTranscript(msg.content)
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
        <div v-if="persistedSteps(msg).length" class="mb-2 space-y-2">
          <div
            v-for="(step, si) in persistedSteps(msg)"
            :key="persistedStepKey(msg.id, si)"
            :data-testid="`persisted-step-${msg.id}-${si}`"
            class="bg-background mr-auto max-w-[90%] rounded-lg border border-border overflow-hidden"
          >
            <button
              class="w-full flex items-center gap-2 px-3 py-2 hover:bg-muted/50 transition-colors text-left"
              @click="toggleToolCall(persistedStepKey(msg.id, si))"
            >
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

              <Wrench v-if="isToolStep(step)" :size="12" class="text-muted-foreground shrink-0" />
              <Activity v-else :size="12" class="text-muted-foreground shrink-0" />
              <span class="font-mono truncate flex-1">{{ step.displayName || step.name }}</span>
              <span
                v-if="formatDurationLabel(msg.execution?.steps?.[si]?.duration_ms)"
                class="text-muted-foreground"
              >
                {{ formatDurationLabel(msg.execution?.steps?.[si]?.duration_ms) }}
              </span>

              <Button
                v-if="canViewStep(step)"
                :data-testid="`persisted-step-view-${msg.id}-${si}`"
                variant="ghost"
                size="sm"
                class="h-5 px-1.5 text-[10px] gap-1"
                @click.stop="emit('viewToolResult', step)"
              >
                <PanelRight :size="10" />
                View
              </Button>

              <ChevronDown
                v-if="expandedToolCalls.has(persistedStepKey(msg.id, si))"
                :size="12"
                class="text-muted-foreground shrink-0"
              />
              <ChevronRight v-else :size="12" class="text-muted-foreground shrink-0" />
            </button>

            <div
              v-if="expandedToolCalls.has(persistedStepKey(msg.id, si)) && step.result"
              class="px-3 py-2 border-t border-border bg-muted/30"
            >
              <pre
                class="text-[11px] font-mono whitespace-pre-wrap break-words max-h-48 overflow-auto"
                >{{ step.result }}</pre
              >
            </div>
          </div>
        </div>

        <div
          :data-testid="`chat-message-${msg.id}`"
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
          <!-- Voice message: show audio player/loading + transcript -->
          <div v-if="getVoiceFilePath(msg)" class="space-y-2">
            <VoiceMessageBubble
              v-if="getVoiceAudio(msg)"
              :blob-url="getVoiceAudio(msg)!.blobUrl"
              :duration="getVoiceAudio(msg)!.duration"
            />
            <div
              v-else-if="loadingMediaPaths.has(getVoiceFilePath(msg)!)"
              class="flex items-center gap-2 text-xs text-muted-foreground py-1"
            >
              <Loader2 :size="12" class="animate-spin" />
              Loading voice message...
            </div>
            <div v-else class="text-xs text-muted-foreground py-1">Voice message unavailable.</div>

            <div
              v-if="getVoiceTranscript(msg)"
              class="text-sm leading-relaxed whitespace-pre-wrap text-foreground"
            >
              {{ getVoiceTranscript(msg) }}
            </div>
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
            v-if="props.enableCopyAction && msg.content"
            variant="outline"
            size="sm"
            class="h-6 px-2 text-[10px] bg-background"
            @click="copyMessage(msg.content)"
          >
            <Copy :size="10" class="mr-1" />
            Copy
          </Button>
          <Button
            v-if="props.enableRegenerateAction && isLastAssistantMessage(idx) && !isStreaming"
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

        <!-- Tool call steps -->
        <div v-if="steps.length > 0" class="mt-3 space-y-1">
          <div
            v-for="(step, idx) in steps"
            :key="idx"
            :data-testid="`stream-step-${idx}`"
            class="text-xs border border-border rounded-md overflow-hidden"
          >
            <!-- Tool call header -->
            <button
              class="w-full flex items-center gap-2 px-2 py-1.5 hover:bg-muted/50 transition-colors"
              @click="toggleToolCall(streamStepKey(idx))"
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
              <span class="font-mono truncate flex-1 text-left">{{
                step.displayName || step.name
              }}</span>

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
                v-if="expandedToolCalls.has(streamStepKey(idx))"
                :size="12"
                class="text-muted-foreground shrink-0"
              />
              <ChevronRight v-else :size="12" class="text-muted-foreground shrink-0" />
            </button>

            <!-- Expanded result -->
            <div
              v-if="expandedToolCalls.has(streamStepKey(idx)) && step.result"
              class="px-2 py-1.5 border-t border-border bg-muted/30"
            >
              <pre
                class="text-[11px] font-mono whitespace-pre-wrap break-words max-h-48 overflow-auto"
                >{{ step.result }}</pre
              >
            </div>
          </div>
        </div>

        <!-- Streaming content -->
        <StreamingMarkdown
          v-if="streamContent"
          :content="streamContent"
          :is-streaming="isStreaming"
          :class="{ 'mt-3': steps.length > 0 }"
        />
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
