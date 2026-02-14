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
import AgentStatusBadge from './AgentStatusBadge.vue'
import AgentOverviewOverlay from './AgentOverviewOverlay.vue'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useBackgroundAgentStream } from '@/composables/workspace/useBackgroundAgentStream'
import { getBackgroundAgentEvents, steerTask } from '@/api/background-agents'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { TaskEvent } from '@/types/generated/TaskEvent'

const props = defineProps<{
  agent: BackgroundAgent
}>()

const emit = defineEmits<{
  refresh: []
}>()

const store = useBackgroundAgentStore()

// Overlay toggle
const showOverview = ref(false)

// Steer input
const steerInput = ref('')
const isSteering = ref(false)

// Event history
const events = ref<TaskEvent[]>([])
const isLoadingEvents = ref(false)

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

// Auto-scroll on new output or events
watch([outputText, () => events.value.length], () => {
  nextTick(scrollToBottom)
})

// Reload events when agent changes
watch(
  () => props.agent.id,
  () => {
    streamTaskId.value = null
    reset()
    loadEvents()
  },
)

onMounted(() => {
  loadEvents()
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
      <div class="max-w-[48rem] mx-auto space-y-3">
        <!-- Loading -->
        <div v-if="isLoadingEvents" class="flex items-center justify-center py-8">
          <Loader2 :size="20" class="animate-spin text-muted-foreground" />
        </div>

        <!-- Event history (rendered as system messages) -->
        <div v-for="event in events" :key="event.id" class="flex items-start gap-2 text-sm">
          <component
            :is="eventIcon(event.event_type)"
            :size="14"
            :class="['mt-0.5 shrink-0', eventColor(event.event_type)]"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-baseline gap-2">
              <span :class="['font-medium', eventColor(event.event_type)]">
                {{ formatEventType(event.event_type) }}
              </span>
              <span class="text-xs text-muted-foreground">
                {{ new Date(event.timestamp).toLocaleString() }}
              </span>
            </div>
            <div v-if="event.message" class="text-xs text-muted-foreground mt-0.5">
              {{ event.message }}
            </div>
            <div
              v-if="event.duration_ms != null || event.tokens_used != null"
              class="text-xs text-muted-foreground mt-0.5"
            >
              <template v-if="event.duration_ms != null">
                {{ formatDuration(event.duration_ms) }}
              </template>
              <template v-if="event.duration_ms != null && event.tokens_used != null"> · </template>
              <template v-if="event.tokens_used != null"> {{ event.tokens_used }} tokens </template>
              <template v-if="event.cost_usd != null">
                · ${{ event.cost_usd.toFixed(4) }}
              </template>
            </div>
            <!-- Output content (for completion events) -->
            <pre
              v-if="event.output"
              class="mt-1 text-xs font-mono bg-muted/30 rounded-md px-2 py-1.5 whitespace-pre-wrap break-words max-h-48 overflow-auto"
              >{{ event.output }}</pre
            >
          </div>
        </div>

        <!-- Live stream output block -->
        <div v-if="streamTaskId && (outputText || isStreaming)" class="bg-muted rounded-lg p-4">
          <div class="text-xs text-muted-foreground mb-1 flex items-center gap-1.5">
            <Loader2 v-if="isStreaming" :size="10" class="animate-spin" />
            <CheckCircle v-else-if="streamState.completedAt" :size="10" class="text-green-500" />
            <AlertCircle v-else-if="streamState.error" :size="10" class="text-destructive" />
            <span>Live Output</span>
            <span v-if="streamState.durationMs" class="ml-auto">
              {{ formatDuration(streamState.durationMs) }}
            </span>
          </div>
          <pre
            v-if="outputText"
            class="text-xs font-mono whitespace-pre-wrap break-words max-h-[60vh] overflow-auto"
            >{{ outputText }}</pre
          >
          <div v-else-if="isStreaming" class="text-xs text-muted-foreground italic">
            Waiting for output...
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

        <!-- Empty state with quick info -->
        <div
          v-if="events.length === 0 && !isLoadingEvents && !streamTaskId"
          class="flex flex-col items-center justify-center py-12 text-muted-foreground"
        >
          <Cog :size="32" class="mb-3 opacity-50" />
          <p class="text-sm">No execution events recorded</p>
          <p class="text-xs mt-1 mb-4">
            Events are recorded when the daemon executes this agent
          </p>
          <!-- Quick stats even when no events -->
          <div
            v-if="agent.success_count > 0 || agent.failure_count > 0 || agent.last_run_at"
            class="text-xs space-y-1 text-center"
          >
            <p v-if="agent.last_run_at">
              Last run: {{ new Date(agent.last_run_at).toLocaleString() }}
            </p>
            <p v-if="agent.success_count > 0 || agent.failure_count > 0">
              <span class="text-green-500">{{ agent.success_count }} success</span>
              /
              <span class="text-destructive">{{ agent.failure_count }} failed</span>
            </p>
          </div>
        </div>
      </div>
    </div>

    <!-- Steer Input -->
    <div v-if="canSteer" class="shrink-0 px-4 pb-4">
      <div class="max-w-[48rem] mx-auto flex gap-2">
        <input
          v-model="steerInput"
          type="text"
          placeholder="Send instruction to agent..."
          class="flex-1 text-sm px-3 py-2 rounded-lg border border-input bg-background focus:outline-none focus:ring-1 focus:ring-ring"
          @keydown.enter="handleSteer"
        />
        <Button :disabled="!steerInput.trim() || isSteering" @click="handleSteer">
          <Send :size="14" />
        </Button>
      </div>
    </div>

    <!-- Overview Overlay (floating) -->
    <AgentOverviewOverlay :agent="agent" :visible="showOverview" @close="showOverview = false" />
  </div>
</template>
