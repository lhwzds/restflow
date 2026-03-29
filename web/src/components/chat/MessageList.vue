<script setup lang="ts">
/**
 * MessageList Component
 *
 * Renders a unified thread containing messages and execution summary items.
 */
import { computed, ref, watch, nextTick, onMounted } from 'vue'
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
import type { StreamStep } from '@/composables/workspace/useChatStream'
import {
  buildChatThreadItems,
  type ThreadItem,
  type ThreadSelection,
} from './threadItems'
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
    threadItems?: ThreadItem[]
  }>(),
  {
    streamThinking: '',
    steps: () => [],
    enableCopyAction: true,
    enableRegenerateAction: true,
    threadItems: undefined,
  },
)

const emit = defineEmits<{
  viewToolResult: [step: StreamStep]
  regenerate: []
  selectThreadItem: [selection: ThreadSelection]
}>()

const toast = useToast()
const scrollContainer = ref<HTMLElement | null>(null)
const expandedItems = ref<Set<string>>(new Set())
// Tracks failed items the user has explicitly collapsed to override auto-expand
const manuallyCollapsed = ref<Set<string>>(new Set())
// run_group expansion: running=always open, failed=default open, completed=default closed
const expandedGroups = ref<Set<string>>(new Set())
const loadedMediaUrls = ref<Map<string, { blobUrl: string; duration: number }>>(new Map())
const loadingMediaPaths = ref<Set<string>>(new Set())

const renderedItems = computed<ThreadItem[]>(() =>
  props.threadItems ??
  buildChatThreadItems({
    messages: props.messages,
    steps: props.steps,
    isStreaming: props.isStreaming,
    streamContent: props.streamContent,
  }),
)

function isAutoExpanded(item: ThreadItem): boolean {
  return item.status === 'failed' && !!item.body && !manuallyCollapsed.value.has(item.id)
}

function toggleItem(id: string, item?: ThreadItem) {
  // If this item is currently auto-expanded (failed + not manually collapsed),
  // the first click should collapse it by marking it manually collapsed.
  if (item && isAutoExpanded(item) && !expandedItems.value.has(id)) {
    manuallyCollapsed.value.add(id)
    return
  }
  manuallyCollapsed.value.delete(id)
  if (expandedItems.value.has(id)) {
    expandedItems.value.delete(id)
  } else {
    expandedItems.value.add(id)
  }
}

function isExpanded(id: string): boolean {
  return expandedItems.value.has(id)
}

function canExpand(item: ThreadItem): boolean {
  return item.expandable && !!item.body
}

function canInspect(item: ThreadItem): boolean {
  return !!item.selection
}

function emitSelection(item: ThreadItem) {
  if (!item.selection) return
  emit('selectThreadItem', item.selection)
  if (item.selection.kind === 'step' && item.selection.step) {
    emit('viewToolResult', item.selection.step)
  }
}

function isMessageItem(item: ThreadItem): item is ThreadItem & { message: ChatMessage } {
  return item.kind === 'message' && !!item.message
}

function isRunGroup(item: ThreadItem): boolean {
  return item.kind === 'run_group'
}

function isGroupExpanded(item: ThreadItem): boolean {
  // Running groups are always expanded
  if (item.status === 'running') return true
  // Failed groups are expanded by default unless user closed them
  if (item.status === 'failed' || item.status === 'interrupted') {
    return !expandedGroups.value.has(`closed-${item.id}`)
  }
  // Completed groups are collapsed by default unless user opened them
  return expandedGroups.value.has(item.id)
}

function toggleGroup(id: string, item: ThreadItem) {
  if (item.status === 'running') return
  const isFailed = item.status === 'failed' || item.status === 'interrupted'
  if (isFailed) {
    // Toggle the "force closed" marker
    const closedKey = `closed-${id}`
    if (expandedGroups.value.has(closedKey)) {
      expandedGroups.value.delete(closedKey)
    } else {
      expandedGroups.value.add(closedKey)
    }
  } else {
    if (expandedGroups.value.has(id)) {
      expandedGroups.value.delete(id)
    } else {
      expandedGroups.value.add(id)
    }
  }
}

function childKindIcon(kind: string): 'tool' | 'llm' | 'other' {
  if (kind === 'tool_call') return 'tool'
  if (kind === 'llm_call') return 'llm'
  return 'other'
}

function isLastAssistantMessage(messageId: string): boolean {
  for (let i = props.messages.length - 1; i >= 0; i -= 1) {
    if (props.messages[i]?.role === 'assistant') {
      return props.messages[i]?.id === messageId
    }
  }
  return false
}

function itemRoleLabel(item: ThreadItem): string {
  const role = item.message?.role
  if (role === 'user') return 'You'
  if (role === 'assistant') return 'Assistant'
  return 'System'
}

function itemKindLabel(item: ThreadItem): string {
  switch (item.kind) {
    case 'tool_call':
      return 'Tool'
    case 'llm_call':
      return 'LLM'
    case 'model_switch':
      return 'Model Switch'
    case 'lifecycle':
      return 'Lifecycle'
    case 'log_record':
      return 'Log'
    default:
      return 'Event'
  }
}

function itemAccentClass(item: ThreadItem): string {
  if (item.status === 'failed') return 'border-l-red-500'
  switch (item.kind) {
    case 'tool_call': return 'border-l-blue-500'
    case 'llm_call': return 'border-l-purple-500'
    case 'model_switch': return 'border-l-orange-500'
    case 'lifecycle': return 'border-l-zinc-400'
    default: return 'border-l-transparent'
  }
}

function persistedStepIds(item: ThreadItem): { row: string; action: string } | null {
  const data = item.selection?.data
  if (!data || data.persisted_execution_step !== true) return null

  const messageId = typeof data.message_id === 'string' ? data.message_id : item.id
  const stepIndex =
    typeof data.step_index === 'number' && Number.isFinite(data.step_index)
      ? data.step_index
      : 0

  return {
    row: `persisted-step-${messageId}-${stepIndex}`,
    action: `persisted-step-view-${messageId}-${stepIndex}`,
  }
}

async function copyMessage(content: string) {
  try {
    await navigator.clipboard.writeText(content)
    toast.success('Copied to clipboard')
  } catch {
    toast.error('Failed to copy')
  }
}

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
  const cached = props.voiceAudioUrls?.get(filePath)
  if (cached) return { ...cached, duration: cached.duration || structuredDuration }
  const loaded = loadedMediaUrls.value.get(filePath)
  if (loaded) return { ...loaded, duration: loaded.duration || structuredDuration }
  if (!loadingMediaPaths.value.has(filePath)) {
    loadingMediaPaths.value.add(filePath)
    void loadMediaFromDisk(filePath)
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
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i)
    }
    const ext = filePath.split('.').pop()?.toLowerCase() ?? 'webm'
    const mimeType = ext === 'ogg' || ext === 'oga' ? 'audio/ogg' : `audio/${ext}`
    const blob = new Blob([bytes], { type: mimeType })
    const blobUrl = URL.createObjectURL(blob)
    loadedMediaUrls.value.set(filePath, { blobUrl, duration: 0 })
  } catch {
    // Ignore unreadable persisted media.
  } finally {
    loadingMediaPaths.value.delete(filePath)
  }
}

function scrollToBottom() {
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight
  }
}

watch(
  () => [renderedItems.value.length, props.streamContent, props.streamThinking],
  async () => {
    await nextTick()
    scrollToBottom()
  },
)

watch(
  renderedItems,
  (items, previousItems) => {
    const hadLiveGroup = previousItems?.some((item) => item.id === 'live-run-group') ?? false
    const hasLiveGroup = items.some((item) => item.id === 'live-run-group')
    if (!hadLiveGroup || hasLiveGroup) return

    for (let index = items.length - 1; index >= 0; index -= 1) {
      const candidate = items[index]
      if (candidate?.kind === 'run_group' && candidate.status === 'completed') {
        expandedGroups.value.add(candidate.id)
        break
      }
    }
  },
  { deep: true },
)

onMounted(() => {
  void nextTick(() => scrollToBottom())
})
</script>

<template>
  <div ref="scrollContainer" class="flex-1 overflow-auto px-4 py-4">
    <div class="mx-auto max-w-[48rem] space-y-4">
      <div v-for="item in renderedItems" :key="item.id" class="group relative">

        <!-- Run group card -->
        <div
          v-if="isRunGroup(item)"
          :data-testid="`run-group-${item.id}`"
          class="mr-auto max-w-[90%] overflow-hidden rounded-lg border border-border bg-background"
        >
          <!-- Group header -->
          <button
            class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50"
            :class="{ 'cursor-default': item.status === 'running' || !item.children?.length }"
            @click="item.children?.length ? toggleGroup(item.id, item) : undefined"
          >
            <Loader2 v-if="item.status === 'running'" :size="12" class="shrink-0 animate-spin text-primary" />
            <Check v-else-if="item.status === 'completed'" :size="12" class="shrink-0 text-green-500" />
            <X v-else :size="12" class="shrink-0 text-red-500" />

            <span class="text-xs font-medium text-foreground/80">
              {{ item.title || 'Turn' }}
            </span>
            <span v-if="item.summary" class="text-[11px] text-muted-foreground">
              · {{ item.summary }}
            </span>
            <span v-if="item.durationLabel" class="text-[11px] text-muted-foreground">
              · {{ item.durationLabel }}
            </span>

            <div class="flex-1" />

            <ChevronDown v-if="item.children?.length && isGroupExpanded(item) && item.status !== 'running'" :size="12" class="shrink-0 text-muted-foreground" />
            <ChevronRight v-else-if="item.children?.length && item.status !== 'running'" :size="12" class="shrink-0 text-muted-foreground" />
          </button>

          <!-- Children tree -->
          <div v-if="isGroupExpanded(item) && item.children?.length" class="border-t border-border">
            <div
              v-for="(child, ci) in item.children"
              :key="child.id"
              class="flex items-center gap-2 px-3 py-1.5 text-xs transition-colors hover:bg-muted/30"
              :class="{ 'border-b border-border/40': ci < (item.children?.length ?? 0) - 1 }"
            >
              <!-- Tree connector -->
              <span class="shrink-0 text-border select-none font-mono text-[10px]">
                {{ ci === (item.children?.length ?? 0) - 1 ? '└─' : '├─' }}
              </span>

              <!-- Status icon -->
              <Loader2 v-if="child.status === 'running'" :size="10" class="shrink-0 animate-spin text-primary" />
              <Check v-else-if="child.status === 'completed'" :size="10" class="shrink-0 text-green-500" />
              <X v-else-if="child.status === 'failed'" :size="10" class="shrink-0 text-red-500" />
              <Wrench v-else-if="childKindIcon(child.kind) === 'tool'" :size="10" class="shrink-0 text-muted-foreground" />
              <Activity v-else :size="10" class="shrink-0 text-muted-foreground" />

              <!-- Title -->
              <span class="flex-1 truncate font-mono text-[12px]">{{ child.title }}</span>

              <!-- Kind badge -->
              <span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                {{ itemKindLabel(child) }}
              </span>

              <!-- Duration -->
              <span v-if="child.durationLabel" class="shrink-0 text-[11px] text-muted-foreground">
                {{ child.durationLabel }}
              </span>

              <!-- View button -->
              <Button
                v-if="canInspect(child)"
                :data-testid="`run-group-child-view-${child.id}`"
                variant="ghost"
                size="sm"
                class="h-5 shrink-0 gap-1 px-1.5 text-[10px]"
                @click.stop="emitSelection(child)"
              >
                <PanelRight :size="10" />
                View
              </Button>
            </div>
          </div>
        </div>

        <!-- Regular step items (non-grouped, no turn_id) -->
        <div
          v-else-if="!isMessageItem(item)"
          :data-testid="persistedStepIds(item)?.row ?? `thread-item-${item.id}`"
          :class="['bg-background mr-auto max-w-[90%] overflow-hidden rounded-lg border border-border border-l-4', itemAccentClass(item)]"
        >
          <button
            class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50"
            @click="canExpand(item) ? toggleItem(item.id, item) : undefined"
          >
            <Loader2
              v-if="item.status === 'running'"
              :size="12"
              class="shrink-0 animate-spin text-primary"
            />
            <Check
              v-else-if="item.status === 'completed'"
              :size="12"
              class="shrink-0 text-green-500"
            />
            <X
              v-else-if="item.status === 'failed'"
              :size="12"
              class="shrink-0 text-red-500"
            />
            <Activity v-else-if="item.kind !== 'tool_call'" :size="12" class="shrink-0 text-muted-foreground" />
            <Wrench v-else :size="12" class="shrink-0 text-muted-foreground" />

            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="truncate font-mono text-sm">{{ item.title }}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                  {{ itemKindLabel(item) }}
                </span>
              </div>
              <div v-if="item.summary" class="mt-0.5 truncate text-xs text-muted-foreground">
                {{ item.summary }}
              </div>
              <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                <span v-if="item.durationLabel">{{ item.durationLabel }}</span>
                <span v-if="item.timestampLabel">{{ item.timestampLabel }}</span>
              </div>
            </div>

            <Button
              v-if="canInspect(item)"
              :data-testid="persistedStepIds(item)?.action ?? `thread-item-view-${item.id}`"
              variant="ghost"
              size="sm"
              class="h-5 gap-1 px-1.5 text-[10px]"
              @click.stop="emitSelection(item)"
            >
              <PanelRight :size="10" />
              View
            </Button>

            <ChevronDown
              v-if="canExpand(item) && (isExpanded(item.id) || isAutoExpanded(item))"
              :size="12"
              class="shrink-0 text-muted-foreground"
            />
            <ChevronRight
              v-else-if="canExpand(item)"
              :size="12"
              class="shrink-0 text-muted-foreground"
            />
          </button>

          <div
            v-if="canExpand(item) && (isExpanded(item.id) || isAutoExpanded(item))"
            :class="[
              'border-t px-3 py-2 max-h-[32rem] overflow-auto',
              item.status === 'failed'
                ? 'border-red-200 bg-red-500/5 dark:border-red-900'
                : 'border-border bg-muted/30',
            ]"
          >
            <pre class="whitespace-pre-wrap break-words text-[11px] font-mono">{{ item.body }}</pre>
          </div>
        </div>

        <template v-else>
          <div
            :data-testid="`chat-message-${item.message.id}`"
            :class="[
              'rounded-lg p-4',
              item.message.role === 'user'
                ? 'ml-auto max-w-[80%] bg-primary/10'
                : 'mr-auto max-w-[90%] bg-muted',
            ]"
          >
            <div class="mb-1 text-xs text-muted-foreground">
              {{ itemRoleLabel(item) }}
            </div>

            <div v-if="getVoiceFilePath(item.message)" class="space-y-2">
              <VoiceMessageBubble
                v-if="getVoiceAudio(item.message)"
                :blob-url="getVoiceAudio(item.message)!.blobUrl"
                :duration="getVoiceAudio(item.message)!.duration"
              />
              <div
                v-else-if="loadingMediaPaths.has(getVoiceFilePath(item.message)!)"
                class="flex items-center gap-2 py-1 text-xs text-muted-foreground"
              >
                <Loader2 :size="12" class="animate-spin" />
                Loading voice message...
              </div>
              <div v-else class="py-1 text-xs text-muted-foreground">Voice message unavailable.</div>

              <div
                v-if="getVoiceTranscript(item.message)"
                class="whitespace-pre-wrap text-sm leading-relaxed text-foreground"
              >
                {{ getVoiceTranscript(item.message) }}
              </div>
            </div>
            <StreamingMarkdown v-else :content="item.message.content || ''" />
          </div>

          <div
            :class="[
              'absolute -bottom-2 z-10 flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100',
              item.message.role === 'user' ? 'right-2' : 'left-2',
            ]"
          >
            <Button
              v-if="item.selection"
              variant="outline"
              size="sm"
              class="h-6 gap-1 bg-background px-2 text-[10px]"
              @click="emitSelection(item)"
            >
              <PanelRight :size="10" class="mr-1" />
              Details
            </Button>
            <Button
              v-if="enableCopyAction && item.message.content"
              variant="outline"
              size="sm"
              class="h-6 bg-background px-2 text-[10px]"
              @click="copyMessage(item.message.content)"
            >
              <Copy :size="10" class="mr-1" />
              Copy
            </Button>
            <Button
              v-if="enableRegenerateAction && isLastAssistantMessage(item.message.id) && !isStreaming"
              variant="outline"
              size="sm"
              class="h-6 bg-background px-2 text-[10px]"
              @click="emit('regenerate')"
            >
              <RefreshCw :size="10" class="mr-1" />
              Retry
            </Button>
          </div>
        </template>
      </div>

      <div
        v-if="streamThinking && !streamContent"
        class="mr-auto max-w-[90%] rounded-lg border border-dashed border-muted-foreground/30 bg-muted/20 text-sm"
      >
        <div class="flex items-center gap-1.5 border-b border-dashed border-muted-foreground/20 px-3 py-1.5">
          <Loader2 :size="11" class="animate-spin text-muted-foreground" />
          <span class="text-[11px] font-medium text-muted-foreground">Thinking...</span>
        </div>
        <div class="max-h-48 overflow-auto px-3 py-2 italic text-muted-foreground/80">
          {{ streamThinking }}
        </div>
      </div>

      <div
        v-if="isStreaming && !streamContent && !streamThinking"
        class="flex items-center gap-2 p-2 text-muted-foreground"
      >
        <div class="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent" />
        <span class="text-sm">Processing...</span>
      </div>

      <div
        v-if="renderedItems.length === 0 && !isStreaming && !streamContent"
        class="flex flex-col items-center justify-center py-20 text-muted-foreground"
      >
        <MessageSquarePlus :size="32" class="mb-3 opacity-50" />
        <p class="text-sm">Start a new conversation</p>
      </div>
    </div>
  </div>
</template>
