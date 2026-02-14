<script setup lang="ts">
/**
 * BackgroundAgentPanel — Chat-style center panel for background agents.
 *
 * Layout mirrors ChatPanel:
 *   Header → Event/Stream message list → Steer input
 * Overview info is in a floating overlay toggled by the Info button.
 */
import { ref, computed, watch, onMounted, nextTick } from 'vue'
import {
  Play,
  Pause,
  RotateCcw,
  XCircle,
  PanelRight,
  Send,
  Loader2,
  AlertCircle,
  CheckCircle,
  Info,
  Cog,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import StreamingMarkdown from '@/components/shared/StreamingMarkdown.vue'
import AgentStatusBadge from './AgentStatusBadge.vue'
import AgentOverviewOverlay from './AgentOverviewOverlay.vue'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useBackgroundAgentStream } from '@/composables/workspace/useBackgroundAgentStream'
import {
  getBackgroundAgentEvents,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
  steerTask,
} from '@/api/background-agents'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { MemorySession } from '@/types/generated/MemorySession'
import type { TaskEvent } from '@/types/generated/TaskEvent'

const props = defineProps<{
  agent: BackgroundAgent
}>()

const emit = defineEmits<{
  refresh: []
}>()

const store = useBackgroundAgentStore()
const MEMORY_CHUNK_LIMIT = 200
const MEMORY_FALLBACK_SESSION_LIMIT = 20

// Overlay toggle
const showOverview = ref(false)

// Steer input
const steerInput = ref('')
const isSteering = ref(false)

// Event history
const events = ref<TaskEvent[]>([])
const isLoadingEvents = ref(false)

// Persisted long-term memory for this background agent
const memoryChunks = ref<MemoryChunk[]>([])
const memorySessions = ref<MemorySession[]>([])
const isLoadingMemory = ref(false)
const memoryLoadError = ref<string | null>(null)

// Stream
const streamTaskId = ref<string | null>(null)
const scrollContainer = ref<HTMLElement | null>(null)

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

async function handlePause() {
  await store.pauseAgent(props.agent.id)
  emit('refresh')
}

async function handleResume() {
  await store.resumeAgent(props.agent.id)
  emit('refresh')
}

async function handleRun() {
  const response = await store.runAgentNow(props.agent.id)
  if (response) {
    streamTaskId.value = response.task_id
    reset()
    await setupListeners()
  }
}

async function handleCancel() {
  await store.cancelAgent(props.agent.id)
  emit('refresh')
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
  isLoadingEvents.value = true
  try {
    events.value = await getBackgroundAgentEvents(props.agent.id, 100)
  } catch (err) {
    console.error('Failed to load events:', err)
  } finally {
    isLoadingEvents.value = false
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
  memoryLoadError.value = null
  memorySessions.value = []

  if (!hasMemoryPersistence.value) {
    memoryChunks.value = []
    return
  }

  isLoadingMemory.value = true
  try {
    const taskTag = `task:${props.agent.id}`
    const taggedChunks = await listMemoryChunksByTag(taskTag, MEMORY_CHUNK_LIMIT)
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

    memorySessions.value = sessionsToInspect

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

    memoryChunks.value = sortChunksByTime(
      dedupeById(sessionChunks.flat()).filter((chunk) => chunkMatchesTask(chunk)),
    )
  } catch (err) {
    memoryLoadError.value = err instanceof Error ? err.message : 'Failed to load memory'
    memoryChunks.value = []
  } finally {
    isLoadingMemory.value = false
  }
}

function scrollToBottom() {
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight
  }
}

function formatEventType(type: string): string {
  return type.charAt(0).toUpperCase() + type.slice(1).replace(/_/g, ' ')
}

function eventIcon(type: string) {
  switch (type) {
    case 'started':
      return Play
    case 'completed':
      return CheckCircle
    case 'failed':
      return AlertCircle
    case 'paused':
      return Pause
    case 'resumed':
      return RotateCcw
    default:
      return Info
  }
}

function eventColor(type: string): string {
  switch (type) {
    case 'completed':
      return 'text-green-500'
    case 'failed':
      return 'text-destructive'
    case 'started':
      return 'text-primary'
    case 'paused':
      return 'text-yellow-500'
    case 'resumed':
      return 'text-blue-500'
    default:
      return 'text-muted-foreground'
  }
}

function formatDuration(ms: number | null): string {
  if (ms == null) return ''
  return `${(ms / 1000).toFixed(1)}s`
}

function formatRelativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp
  if (diff < 60_000) return 'just now'
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h ago`
  return new Date(timestamp).toLocaleDateString()
}

// Classify events for chat-style rendering
function isSystemEvent(type: string): boolean {
  return ['started', 'completed', 'failed', 'paused', 'resumed', 'cancelled'].includes(type)
}

function eventSummary(event: TaskEvent): string {
  const parts: string[] = []
  if (event.duration_ms != null) parts.push(formatDuration(event.duration_ms))
  if (event.tokens_used != null) parts.push(`${event.tokens_used} tokens`)
  if (event.cost_usd != null) parts.push(`$${event.cost_usd.toFixed(4)}`)
  return parts.join(' · ')
}

// Auto-scroll on new output, events, or loaded memory
watch([outputText, () => events.value.length, () => memoryChunks.value.length], () => {
  nextTick(scrollToBottom)
})

// Reload events when agent changes
watch(
  () => props.agent.id,
  () => {
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
        title="Pause"
        @click="handlePause"
      >
        <Pause :size="12" />
      </Button>
      <Button
        v-if="canResume"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        title="Resume"
        @click="handleResume"
      >
        <RotateCcw :size="12" />
      </Button>
      <Button
        v-if="canRun"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        title="Run now"
        @click="handleRun"
      >
        <Play :size="12" />
      </Button>
      <Button
        v-if="canCancel"
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        title="Cancel"
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
        title="Toggle overview"
        @click="showOverview = !showOverview"
      >
        <PanelRight :size="12" />
      </Button>
    </div>

    <!-- Message / Event List -->
    <div ref="scrollContainer" class="flex-1 overflow-auto px-4 py-4">
      <div class="max-w-[48rem] mx-auto space-y-4">
        <!-- Loading -->
        <div v-if="isLoadingEvents" class="flex items-center gap-2 text-muted-foreground p-2">
          <div
            class="animate-spin h-4 w-4 border-2 border-primary border-t-transparent rounded-full"
          />
          <span class="text-sm">Loading history...</span>
        </div>

        <!-- Event history (chat-style) -->
        <template v-for="event in events" :key="event.id">
          <!-- System events: centered pill -->
          <div v-if="isSystemEvent(event.event_type)" class="flex justify-center">
            <div
              :class="[
                'inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs',
                event.event_type === 'failed'
                  ? 'bg-destructive/10 text-destructive'
                  : event.event_type === 'completed'
                    ? 'bg-green-500/10 text-green-600'
                    : 'bg-muted text-muted-foreground',
              ]"
            >
              <component :is="eventIcon(event.event_type)" :size="11" />
              <span>{{ formatEventType(event.event_type) }}</span>
              <span v-if="eventSummary(event)" class="opacity-70">{{ eventSummary(event) }}</span>
              <span class="opacity-50">{{ formatRelativeTime(event.timestamp) }}</span>
            </div>
          </div>

          <!-- Output events: assistant-style bubble -->
          <div v-else-if="event.output" class="bg-muted mr-auto max-w-[90%] p-4 rounded-lg">
            <div class="text-xs text-muted-foreground mb-1">
              {{ formatEventType(event.event_type) }}
              <span class="opacity-50 ml-1">{{ formatRelativeTime(event.timestamp) }}</span>
            </div>
            <StreamingMarkdown :content="event.output" />
          </div>

          <!-- Other events with message: subtle info row -->
          <div v-else-if="event.message" class="bg-muted mr-auto max-w-[90%] p-3 rounded-lg">
            <div class="text-xs text-muted-foreground mb-0.5 flex items-center gap-1.5">
              <component
                :is="eventIcon(event.event_type)"
                :size="11"
                :class="eventColor(event.event_type)"
              />
              {{ formatEventType(event.event_type) }}
              <span class="opacity-50">{{ formatRelativeTime(event.timestamp) }}</span>
            </div>
            <div class="text-sm">{{ event.message }}</div>
          </div>
        </template>

        <!-- Persisted memory transcript (chat-style) -->
        <template v-if="hasMemoryPersistence && memoryChunks.length > 0">
          <div class="flex justify-center">
            <div class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs bg-muted text-muted-foreground">
              <Info :size="11" />
              <span>Persisted Memory · {{ memoryChunks.length }} chunks</span>
            </div>
          </div>
          <div
            v-for="chunk in memoryChunks"
            :key="chunk.id"
            class="bg-muted/60 mr-auto max-w-[90%] p-3 rounded-lg"
          >
            <div class="text-xs text-muted-foreground mb-0.5">
              {{ new Date(chunk.created_at).toLocaleString() }}
            </div>
            <StreamingMarkdown :content="chunk.content" />
          </div>
        </template>

        <!-- Live stream output (assistant bubble style) -->
        <div
          v-if="streamTaskId && (outputText || isStreaming)"
          class="bg-muted mr-auto max-w-[90%] p-4 rounded-lg"
        >
          <div class="text-xs text-muted-foreground mb-1">Agent</div>

          <!-- Thinking indicator -->
          <div v-if="isStreaming && !outputText" class="text-sm text-muted-foreground italic mb-2">
            Thinking...
          </div>

          <!-- Streaming content -->
          <StreamingMarkdown
            v-if="outputText"
            :content="outputText"
            :is-streaming="isStreaming"
          />

          <!-- Stats footer -->
          <div
            v-if="!isStreaming && streamState.durationMs"
            class="text-[11px] text-muted-foreground mt-2 opacity-60"
          >
            {{ formatDuration(streamState.durationMs) }}
          </div>

          <!-- Error banner -->
          <div
            v-if="streamState.error"
            class="mt-2 text-xs text-destructive bg-destructive/10 rounded px-2 py-1 font-mono break-words"
          >
            {{ streamState.error }}
          </div>

          <!-- Result -->
          <div
            v-if="streamState.result && !isStreaming"
            class="mt-2 text-xs text-green-600 bg-green-500/10 rounded px-2 py-1 break-words"
          >
            {{ streamState.result }}
          </div>
        </div>

        <!-- Typing indicator (matches ChatPanel) -->
        <div
          v-if="isStreaming && !outputText && streamTaskId"
          class="flex items-center gap-2 text-muted-foreground p-2"
        >
          <div
            class="animate-spin h-4 w-4 border-2 border-primary border-t-transparent rounded-full"
          />
          <span class="text-sm">Processing...</span>
        </div>

        <!-- Empty state: only when truly empty (no events, no stats, no stream) -->
        <div
          v-if="
            events.length === 0 &&
            !isLoadingEvents &&
            !streamTaskId &&
            !agent.last_run_at &&
            agent.success_count === 0 &&
            agent.failure_count === 0
          "
          class="flex flex-col items-center justify-center py-20 text-muted-foreground"
        >
          <Cog :size="32" class="mb-3 opacity-50" />
          <p class="text-sm">No executions yet</p>
          <p class="text-xs mt-1">Click Run to start the agent</p>
        </div>

        <!-- Stats summary when events are empty but agent has run history -->
        <div
          v-else-if="events.length === 0 && !isLoadingEvents && !streamTaskId && agent.last_run_at"
          class="flex justify-center"
        >
          <div
            class="inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-xs bg-muted text-muted-foreground"
          >
            <span>
              Last run {{ formatRelativeTime(agent.last_run_at) }}
            </span>
            <span class="opacity-40">·</span>
            <span class="text-green-500">{{ agent.success_count }} passed</span>
            <span class="opacity-40">·</span>
            <span class="text-destructive">{{ agent.failure_count }} failed</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Steer Input (only visible when agent is actively running/streaming) -->
    <div v-if="canSteer" class="shrink-0 px-4 pb-4">
      <div class="max-w-[48rem] mx-auto">
        <div
          class="flex items-end gap-2 rounded-xl border border-input bg-background px-3 py-2 focus-within:ring-1 focus-within:ring-ring"
        >
          <textarea
            v-model="steerInput"
            rows="1"
            placeholder="Send instruction to agent..."
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
