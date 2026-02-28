<script setup lang="ts">
/**
 * BackgroundAgentPanel — Chat-style center panel for background agents.
 *
 * Layout mirrors ChatPanel:
 *   Header → Event/Stream message list → Steer input
 * Overview info is in a floating overlay toggled by the Info button.
 */
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  Play,
  Pause,
  RotateCcw,
  XCircle,
  PanelRight,
  Send,
  Loader2,
  Cog,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import MessageList from '@/components/chat/MessageList.vue'
import AgentStatusBadge from './AgentStatusBadge.vue'
import AgentOverviewOverlay from './AgentOverviewOverlay.vue'
import { useToast } from '@/composables/useToast'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useBackgroundAgentStream } from '@/composables/workspace/useBackgroundAgentStream'
import { buildBackgroundTimelineMessages } from '@/components/conversation/adapters/backgroundTimeline'
import { shouldShowLiveStreamBubble } from './streamVisibility'
import {
  getBackgroundAgentEvents,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
  steerTask,
} from '@/api/background-agents'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import type { ChatMessage } from '@/types/generated/ChatMessage'

const props = defineProps<{
  agent: BackgroundAgent
}>()

const emit = defineEmits<{
  refresh: []
}>()

const { t } = useI18n()
const toast = useToast()
const store = useBackgroundAgentStore()
const MEMORY_CHUNK_LIMIT = 200
const MEMORY_FALLBACK_SESSION_LIMIT = 20

// Overlay toggle
const showOverview = ref(false)

// Steer input
const steerInput = ref('')
const isSteering = ref(false)

// Staleness guard for agent switch race condition
let loadVersion = 0

// Event history
const events = ref<TaskEvent[]>([])
const isLoadingEvents = ref(false)

// Persisted long-term memory for this background agent
const memoryChunks = ref<MemoryChunk[]>([])

// Stream
const streamTaskId = ref<string | null>(null)
const { streamState, isStreaming, outputText, setupListeners, reset } = useBackgroundAgentStream(
  () => streamTaskId.value,
)

// Control button visibility
const canPause = computed(() => props.agent.status === 'active')
const canResume = computed(() => props.agent.status === 'paused')
const canRun = computed(() => props.agent.status === 'active' || props.agent.status === 'paused')
const canCancel = computed(() => props.agent.status === 'running')
const canSteer = computed(() => isStreaming.value || props.agent.status === 'running')
const hasMemoryPersistence = computed(() => props.agent.memory.persist_on_complete)
const showLiveStreamBubble = computed(() =>
  shouldShowLiveStreamBubble({
    streamTaskId: streamTaskId.value,
    isStreaming: isStreaming.value,
    outputText: outputText.value,
    events: events.value,
  }),
)
const timelineMessages = computed<ChatMessage[]>(() =>
  buildBackgroundTimelineMessages({
    events: events.value,
    memoryChunks: hasMemoryPersistence.value ? memoryChunks.value : [],
  }),
)
const timelineSteps = computed<StreamStep[]>(() => [])
const showInitialEmptyState = computed(
  () =>
    timelineMessages.value.length === 0 &&
    !isLoadingEvents.value &&
    !showLiveStreamBubble.value &&
    !props.agent.last_run_at &&
    props.agent.success_count === 0 &&
    props.agent.failure_count === 0,
)
const showStatsSummary = computed(
  () =>
    timelineMessages.value.length === 0 &&
    !isLoadingEvents.value &&
    !showLiveStreamBubble.value &&
    !!props.agent.last_run_at,
)

async function handlePause() {
  await store.pauseAgent(props.agent.id)
  if (store.error) {
    toast.error(store.error)
  } else {
    emit('refresh')
  }
}

async function handleResume() {
  await store.resumeAgent(props.agent.id)
  if (store.error) {
    toast.error(store.error)
  } else {
    emit('refresh')
  }
}

async function handleRun() {
  const response = await store.runAgentNow(props.agent.id)
  if (store.error) {
    toast.error(store.error)
    return
  }
  if (response) {
    streamTaskId.value = response.task_id
    reset()
    await setupListeners()
  }
}

async function handleCancel() {
  await store.cancelAgent(props.agent.id)
  if (store.error) {
    toast.error(store.error)
  } else {
    emit('refresh')
  }
}

async function handleSteer() {
  const instruction = steerInput.value.trim()
  const taskId = streamTaskId.value
  if (!instruction || !taskId) return

  isSteering.value = true
  try {
    await steerTask(taskId, instruction)
    steerInput.value = ''
  } catch (err) {
    console.error('Failed to steer task:', err)
  } finally {
    isSteering.value = false
  }
}

async function loadEvents() {
  const version = loadVersion
  isLoadingEvents.value = true
  try {
    const result = await getBackgroundAgentEvents(props.agent.id, 100)
    if (version !== loadVersion) return
    events.value = result
  } catch (err) {
    console.error('Failed to load events:', err)
  } finally {
    if (version === loadVersion) {
      isLoadingEvents.value = false
    }
  }
}

function memoryAgentIdCandidates(): string[] {
  const sharedNamespace = props.agent.agent_id
  const taskNamespace = `${props.agent.agent_id}::task::${props.agent.id}`
  const ordered =
    props.agent.memory.memory_scope === 'per_background_agent'
      ? [taskNamespace, sharedNamespace]
      : [sharedNamespace, taskNamespace]
  return [...new Set(ordered)]
}

function dedupeById<T extends { id: string }>(items: T[]): T[] {
  const map = new Map<string, T>()
  for (const item of items) {
    map.set(item.id, item)
  }
  return Array.from(map.values())
}

function chunkMatchesTask(chunk: MemoryChunk): boolean {
  if (chunk.source.type === 'task_execution' && chunk.source.task_id === props.agent.id) {
    return true
  }
  return chunk.tags.includes(`task:${props.agent.id}`)
}

function sortChunksByTime(chunks: MemoryChunk[]): MemoryChunk[] {
  return [...chunks].sort((a, b) => a.created_at - b.created_at)
}

async function loadMemoryConversation() {
  const version = loadVersion

  if (!hasMemoryPersistence.value) {
    memoryChunks.value = []
    return
  }

  try {
    const taskTag = `task:${props.agent.id}`
    const taggedChunks = await listMemoryChunksByTag(taskTag, MEMORY_CHUNK_LIMIT)
    if (version !== loadVersion) return
    const filteredTaggedChunks = sortChunksByTime(dedupeById(taggedChunks).filter(chunkMatchesTask))

    if (filteredTaggedChunks.length > 0) {
      memoryChunks.value = filteredTaggedChunks
      return
    }

    // Fallback for older records that may not have task tags.
    const sessionsPerNamespace = await Promise.all(
      memoryAgentIdCandidates().map(async (agentId) => {
        try {
          return await listMemorySessions(agentId)
        } catch (err) {
          console.warn(`Failed to load memory sessions for namespace ${agentId}:`, err)
          return []
        }
      }),
    )
    if (version !== loadVersion) return

    const allSessions = dedupeById(sessionsPerNamespace.flat()).sort(
      (a, b) => b.updated_at - a.updated_at,
    )
    const taggedSessions = allSessions.filter((session) => session.tags.includes(taskTag))
    const sessionsToInspect =
      taggedSessions.length > 0
        ? taggedSessions
        : allSessions.slice(0, MEMORY_FALLBACK_SESSION_LIMIT)

    if (sessionsToInspect.length === 0) {
      memoryChunks.value = []
      return
    }

    const sessionChunks = await Promise.all(
      sessionsToInspect.map(async (session) => {
        try {
          return await listMemoryChunksForSession(session.id)
        } catch (err) {
          console.warn(`Failed to load memory chunks for session ${session.id}:`, err)
          return []
        }
      }),
    )
    if (version !== loadVersion) return

    memoryChunks.value = sortChunksByTime(
      dedupeById(sessionChunks.flat()).filter((chunk) => chunkMatchesTask(chunk)),
    )
  } catch (err) {
    if (version !== loadVersion) return
    console.warn('Failed to load memory conversation:', err)
    memoryChunks.value = []
  }
}

function formatRelativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp
  if (diff < 60_000) return 'just now'
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h ago`
  return new Date(timestamp).toLocaleDateString()
}

// Reload events when agent changes
watch(
  () => props.agent.id,
  () => {
    loadVersion++
    streamTaskId.value = null
    reset()
    loadEvents()
    loadMemoryConversation()
  },
)

watch(
  () => streamState.value.completedAt,
  (completedAt, previousCompletedAt) => {
    if (completedAt && completedAt !== previousCompletedAt) {
      loadEvents()
      loadMemoryConversation()
    }
  },
)

onMounted(() => {
  loadEvents()
  loadMemoryConversation()
})
</script>

<template>
  <div class="relative flex-1 flex flex-col min-w-0 overflow-hidden">
    <!-- Header (mirrors ChatHeader style) -->
    <div
      class="flex items-center gap-2 px-3 py-1.5 border-b border-border shrink-0 text-xs text-muted-foreground"
      data-tauri-drag-region
    >
      <Cog :size="12" class="text-blue-500 shrink-0" />
      <span class="font-medium text-foreground truncate">{{ agent.name }}</span>
      <AgentStatusBadge :status="agent.status" />

      <!-- Streaming indicator -->
      <Loader2 v-if="isStreaming" :size="12" class="animate-spin text-primary shrink-0" />

      <!-- Progress phase -->
      <span v-if="streamState.phase && isStreaming" class="truncate">
        {{ streamState.phase }}
      </span>

      <div class="flex-1" />

      <!-- Control buttons -->
      <Button
        v-if="canPause"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.pause')"
        @click="handlePause"
      >
        <Pause :size="12" />
      </Button>
      <Button
        v-if="canResume"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.resume')"
        @click="handleResume"
      >
        <RotateCcw :size="12" />
      </Button>
      <Button
        v-if="canRun"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.runNow')"
        @click="handleRun"
      >
        <Play :size="12" />
      </Button>
      <Button
        v-if="canCancel"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :title="t('backgroundAgent.cancel')"
        @click="handleCancel"
      >
        <XCircle :size="12" />
      </Button>

      <!-- Info toggle -->
      <Button
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :class="showOverview && 'bg-muted'"
        :title="t('backgroundAgent.toggleOverview')"
        @click="showOverview = !showOverview"
      >
        <PanelRight :size="12" />
      </Button>
    </div>

    <!-- Message Timeline (shared with chat session MessageList) -->
    <div class="relative flex-1 min-h-0">
      <MessageList
        :messages="timelineMessages"
        :is-streaming="isStreaming"
        :stream-content="showLiveStreamBubble ? outputText : ''"
        :stream-thinking="showLiveStreamBubble ? t('backgroundAgent.thinking') : ''"
        :steps="timelineSteps"
        :enable-copy-action="false"
        :enable-regenerate-action="false"
      />

      <div
        v-if="showInitialEmptyState"
        class="absolute inset-0 flex flex-col items-center justify-center text-muted-foreground pointer-events-none"
      >
        <Cog :size="32" class="mb-3 opacity-50" />
        <p class="text-sm">{{ t('backgroundAgent.noExecutions') }}</p>
        <p class="text-xs mt-1">{{ t('backgroundAgent.clickRunToStart') }}</p>
      </div>

      <div v-else-if="showStatsSummary" class="absolute inset-x-0 bottom-6 flex justify-center pointer-events-none">
        <div
          class="inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-xs bg-muted text-muted-foreground"
        >
          <span>
            {{ t('backgroundAgent.lastRun', { time: formatRelativeTime(agent.last_run_at!) }) }}
          </span>
          <span class="opacity-40">·</span>
          <span class="text-green-500">{{ t('backgroundAgent.passed', { count: agent.success_count }) }}</span>
          <span class="opacity-40">·</span>
          <span class="text-destructive">{{ t('backgroundAgent.failed', { count: agent.failure_count }) }}</span>
        </div>
      </div>

      <div
        v-if="isLoadingEvents"
        class="absolute left-4 top-4 inline-flex items-center gap-2 rounded-md border border-border bg-background/90 px-2 py-1 text-xs text-muted-foreground pointer-events-none"
      >
        <Loader2 :size="12" class="animate-spin" />
        <span>{{ t('backgroundAgent.loadingHistory') }}</span>
      </div>
    </div>

    <!-- Steer Input (only visible when agent is actively running/streaming) -->
    <div v-show="canSteer" class="shrink-0 px-4 pb-4">
      <div class="max-w-[48rem] mx-auto">
        <div
          class="flex items-end gap-2 rounded-xl border border-input bg-background px-3 py-2 focus-within:ring-1 focus-within:ring-ring"
        >
          <textarea
            v-model="steerInput"
            rows="1"
            :placeholder="t('backgroundAgent.steerPlaceholder')"
            class="flex-1 resize-none bg-transparent text-sm outline-none placeholder:text-muted-foreground max-h-32"
            @keydown.enter.exact.prevent="handleSteer"
          />
          <Button
            size="icon"
            class="h-7 w-7 shrink-0"
            :disabled="!steerInput.trim() || isSteering"
            @click="handleSteer"
          >
            <Loader2 v-if="isSteering" :size="14" class="animate-spin" />
            <Send v-else :size="14" />
          </Button>
        </div>
      </div>
    </div>

    <!-- Overview Overlay (floating) -->
    <AgentOverviewOverlay :agent="agent" :visible="showOverview" @close="showOverview = false" />
  </div>
</template>
