<script setup lang="ts">
/**
 * Task Execution Panel Component
 *
 * Displays real-time task execution with streaming output,
 * progress tracking, and status indicators.
 */
import { ref, computed, watch, nextTick, onMounted, toRef } from 'vue'
import {
  Play,
  Square,
  Terminal,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Clock,
  Loader2,
  ChevronDown,
  ChevronUp,
  Copy,
  Check,
  Cpu,
  FileText,
  Zap,
} from 'lucide-vue-next'
import { useBackgroundAgentStreamEvents } from '@/composables/agents/useBackgroundAgentStreamEvents'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader } from '@/components/ui/card'

const props = withDefaults(
  defineProps<{
    /** Task ID to monitor */
    taskId: string
    /** Whether to auto-start listening on mount */
    autoStart?: boolean
    /** Whether to auto-scroll output */
    autoScroll?: boolean
    /** Maximum height for output area (CSS value) */
    maxHeight?: string
    /** Show compact view without card wrapper */
    compact?: boolean
  }>(),
  {
    autoStart: true,
    autoScroll: true,
    maxHeight: '400px',
    compact: false,
  },
)

const emit = defineEmits<{
  /** Emitted when task completes */
  (e: 'completed', result: string | null): void
  /** Emitted when task fails */
  (e: 'failed', error: string | null): void
  /** Emitted when task is cancelled */
  (e: 'cancelled'): void
}>()

// Create a ref for taskId
const taskIdRef = toRef(() => props.taskId)

// Use the task stream events composable
const {
  state,
  isRunning,
  isCompleted,
  isFailed,
  isCancelled,
  isFinished,
  combinedOutput,
  outputLineCount,
  startListening,
  stopListening,
  runTask,
  cancel,
} = useBackgroundAgentStreamEvents(taskIdRef)

// Local state
const outputRef = ref<HTMLElement | null>(null)
const isOutputExpanded = ref(true)
const copied = ref(false)

// Computed properties
const statusIcon = computed(() => {
  if (!state.value) return Clock
  switch (state.value.status) {
    case 'pending':
      return Clock
    case 'running':
      return Loader2
    case 'completed':
      return CheckCircle2
    case 'failed':
      return XCircle
    case 'cancelled':
      return AlertCircle
    default:
      return Clock
  }
})

const statusBadgeVariant = computed(
  (): 'default' | 'success' | 'destructive' | 'warning' | 'info' => {
    if (!state.value) return 'default'
    switch (state.value.status) {
      case 'pending':
        return 'default'
      case 'running':
        return 'info'
      case 'completed':
        return 'success'
      case 'failed':
        return 'destructive'
      case 'cancelled':
        return 'warning'
      default:
        return 'default'
    }
  },
)

const statusText = computed(() => {
  if (!state.value) return 'Pending'
  switch (state.value.status) {
    case 'pending':
      return 'Pending'
    case 'running':
      return 'Running'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
    case 'cancelled':
      return 'Cancelled'
    default:
      return 'Unknown'
  }
})

const formattedDuration = computed(() => {
  if (!state.value?.durationMs) return '0s'
  const ms = state.value.durationMs
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  const minutes = Math.floor(ms / 60000)
  const seconds = ((ms % 60000) / 1000).toFixed(0)
  return `${minutes}m ${seconds}s`
})

const hasOutput = computed(() => {
  return combinedOutput.value.length > 0
})

const hasProgress = computed(() => {
  return state.value?.progressPercent !== null && state.value?.progressPercent !== undefined
})

// Watch for task completion events
watch(
  () => state.value?.status,
  (newStatus, oldStatus) => {
    if (newStatus === oldStatus) return

    if (newStatus === 'completed') {
      emit('completed', state.value?.result ?? null)
    } else if (newStatus === 'failed') {
      emit('failed', state.value?.error ?? null)
    } else if (newStatus === 'cancelled') {
      emit('cancelled')
    }
  },
)

// Auto-scroll output
watch(combinedOutput, async () => {
  if (props.autoScroll && outputRef.value) {
    await nextTick()
    outputRef.value.scrollTop = outputRef.value.scrollHeight
  }
})

// Start listening on mount if autoStart
onMounted(() => {
  if (props.autoStart) {
    startListening()
  }
})

// Actions
async function handleRunTask() {
  try {
    await runTask()
  } catch (error) {
    console.error('Failed to start task:', error)
  }
}

async function handleCancel() {
  try {
    await cancel()
  } catch (error) {
    console.error('Failed to cancel task:', error)
  }
}

async function copyOutput() {
  if (!combinedOutput.value) return
  try {
    await navigator.clipboard.writeText(combinedOutput.value)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch (error) {
    console.error('Failed to copy output:', error)
  }
}

function toggleOutput() {
  isOutputExpanded.value = !isOutputExpanded.value
}

// Helper function for formatting bytes
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

// Expose methods for parent components
defineExpose({
  startListening,
  stopListening,
  runTask: handleRunTask,
  cancel: handleCancel,
  state,
})
</script>

<template>
  <component :is="compact ? 'div' : Card" :class="['task-execution-panel', { compact }]">
    <component :is="compact ? 'div' : CardHeader" class="panel-header">
      <div class="header-content">
        <div class="status-section">
          <component
            :is="statusIcon"
            :size="18"
            :class="['status-icon', state?.status, { spinning: isRunning }]"
          />
          <Badge :variant="statusBadgeVariant">
            {{ statusText }}
          </Badge>
          <span v-if="state?.executionMode" class="execution-mode">
            <Cpu :size="14" />
            {{ state.executionMode }}
          </span>
        </div>

        <div class="actions">
          <Button
            v-if="!isRunning && !isFinished"
            size="sm"
            variant="default"
            @click="handleRunTask"
          >
            <Play :size="14" />
            Run
          </Button>
          <Button v-if="isRunning" size="sm" variant="destructive" @click="handleCancel">
            <Square :size="14" />
            Cancel
          </Button>
          <Button
            v-if="hasOutput"
            size="sm"
            variant="ghost"
            @click="copyOutput"
            :title="copied ? 'Copied!' : 'Copy output'"
          >
            <component :is="copied ? Check : Copy" :size="14" />
          </Button>
          <Button
            v-if="hasOutput"
            size="sm"
            variant="ghost"
            @click="toggleOutput"
            :title="isOutputExpanded ? 'Collapse output' : 'Expand output'"
          >
            <component :is="isOutputExpanded ? ChevronUp : ChevronDown" :size="14" />
          </Button>
        </div>
      </div>

      <!-- Task info -->
      <div v-if="state?.taskName || state?.agentId" class="task-info">
        <span v-if="state.taskName" class="task-name">{{ state.taskName }}</span>
        <span v-if="state.agentId" class="agent-id">Agent: {{ state.agentId }}</span>
      </div>

      <!-- Progress bar -->
      <div v-if="hasProgress" class="progress-section">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: `${state?.progressPercent ?? 0}%` }" />
        </div>
        <span class="progress-text">
          <span v-if="state?.progressPhase">{{ state.progressPhase }} - </span>
          {{ state?.progressPercent ?? 0 }}%
        </span>
      </div>

      <!-- Duration -->
      <div class="meta-info">
        <span class="duration">
          <Clock :size="14" />
          {{ formattedDuration }}
        </span>
        <span v-if="outputLineCount > 0" class="line-count">
          <FileText :size="14" />
          {{ outputLineCount }} lines
        </span>
      </div>
    </component>

    <component :is="compact ? 'div' : CardContent" v-if="isOutputExpanded" class="panel-content">
      <!-- Output terminal -->
      <div ref="outputRef" class="output-terminal" :style="{ maxHeight: maxHeight }">
        <div v-if="!hasOutput && !isRunning" class="output-placeholder">
          <Terminal :size="24" />
          <span>Output will appear here when task runs...</span>
        </div>
        <div v-else-if="!hasOutput && isRunning" class="output-placeholder">
          <Loader2 :size="24" class="spinning" />
          <span>Waiting for output...</span>
        </div>
        <pre
          v-else
          class="output-content"
        ><template v-for="(line, idx) in state?.outputLines" :key="idx"><span :class="{ stderr: line.isStderr }">{{ line.text }}</span></template></pre>
      </div>

      <!-- Result / Error display -->
      <div v-if="isCompleted && state?.result" class="result-section success">
        <CheckCircle2 :size="16" />
        <div class="result-content">
          <strong>Result:</strong>
          <p>{{ state.result }}</p>
        </div>
      </div>

      <div v-if="isFailed && state?.error" class="result-section error">
        <XCircle :size="16" />
        <div class="result-content">
          <strong>Error:</strong>
          <p>{{ state.error }}</p>
        </div>
      </div>

      <div v-if="isCancelled" class="result-section warning">
        <AlertCircle :size="16" />
        <div class="result-content">
          <strong>Cancelled</strong>
          <p v-if="state?.error">{{ state.error }}</p>
        </div>
      </div>

      <!-- Execution stats -->
      <div v-if="isFinished && state?.stats" class="stats-section">
        <Zap :size="14" />
        <span class="stats-title">Execution Stats</span>
        <div class="stats-grid">
          <div v-if="state.stats.output_lines !== null" class="stat-item">
            <span class="stat-label">Output Lines</span>
            <span class="stat-value">{{ state.stats.output_lines }}</span>
          </div>
          <div v-if="state.stats.output_bytes !== null" class="stat-item">
            <span class="stat-label">Output Size</span>
            <span class="stat-value">{{ formatBytes(Number(state.stats.output_bytes)) }}</span>
          </div>
          <div v-if="state.stats.api_calls !== null" class="stat-item">
            <span class="stat-label">API Calls</span>
            <span class="stat-value">{{ state.stats.api_calls }}</span>
          </div>
          <div v-if="state.stats.tokens_used !== null" class="stat-item">
            <span class="stat-label">Tokens Used</span>
            <span class="stat-value">{{ state.stats.tokens_used.toLocaleString() }}</span>
          </div>
        </div>
      </div>
    </component>
  </component>
</template>

<style lang="scss" scoped>
.task-execution-panel {
  &:not(.compact) {
    border: 1px solid var(--rf-color-border-base);
    border-radius: var(--rf-radius-medium);
  }

  .panel-header {
    padding: var(--rf-spacing-md);
    border-bottom: 1px solid var(--rf-color-border-base);

    .header-content {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--rf-spacing-md);
    }

    .status-section {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-sm);

      .status-icon {
        &.pending {
          color: var(--rf-color-text-secondary);
        }
        &.running {
          color: hsl(var(--info));
        }
        &.completed {
          color: hsl(var(--success, 142 76% 36%));
        }
        &.failed {
          color: hsl(var(--destructive));
        }
        &.cancelled {
          color: hsl(var(--warning, 48 96% 53%));
        }
        &.spinning {
          animation: spin 1s linear infinite;
        }
      }

      .execution-mode {
        display: flex;
        align-items: center;
        gap: var(--rf-spacing-xs);
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
        background: var(--rf-color-bg-secondary);
        border-radius: var(--rf-radius-small);
      }
    }

    .actions {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
    }

    .task-info {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-md);
      margin-top: var(--rf-spacing-sm);
      font-size: var(--rf-font-size-sm);

      .task-name {
        font-weight: var(--rf-font-weight-semibold);
        color: var(--rf-color-text-primary);
      }

      .agent-id {
        color: var(--rf-color-text-secondary);
      }
    }

    .progress-section {
      margin-top: var(--rf-spacing-sm);
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-sm);

      .progress-bar {
        flex: 1;
        height: 6px;
        background: var(--rf-color-bg-secondary);
        border-radius: 3px;
        overflow: hidden;

        .progress-fill {
          height: 100%;
          background: hsl(var(--primary));
          transition: width 0.3s ease;
        }
      }

      .progress-text {
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        min-width: 60px;
        text-align: right;
      }
    }

    .meta-info {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-md);
      margin-top: var(--rf-spacing-sm);
      font-size: var(--rf-font-size-xs);
      color: var(--rf-color-text-secondary);

      .duration,
      .line-count {
        display: flex;
        align-items: center;
        gap: var(--rf-spacing-xs);
      }
    }
  }

  .panel-content {
    padding: var(--rf-spacing-md);

    .output-terminal {
      background: var(--rf-color-bg-base);
      border: 1px solid var(--rf-color-border-base);
      border-radius: var(--rf-radius-small);
      overflow: auto;
      font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
      font-size: var(--rf-font-size-xs);
      line-height: 1.5;

      .output-placeholder {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--rf-spacing-sm);
        padding: var(--rf-spacing-xl);
        color: var(--rf-color-text-secondary);

        svg.spinning {
          animation: spin 1s linear infinite;
        }
      }

      .output-content {
        padding: var(--rf-spacing-sm);
        margin: 0;
        white-space: pre-wrap;
        word-break: break-all;
        color: var(--rf-color-text-primary);

        .stderr {
          color: hsl(var(--destructive));
        }
      }
    }

    .result-section {
      display: flex;
      gap: var(--rf-spacing-sm);
      margin-top: var(--rf-spacing-md);
      padding: var(--rf-spacing-md);
      border-radius: var(--rf-radius-small);

      &.success {
        background: hsla(var(--success, 142 76% 36%), 0.1);
        border: 1px solid hsla(var(--success, 142 76% 36%), 0.3);

        svg {
          color: hsl(var(--success, 142 76% 36%));
          flex-shrink: 0;
        }
      }

      &.error {
        background: hsla(var(--destructive), 0.1);
        border: 1px solid hsla(var(--destructive), 0.3);

        svg {
          color: hsl(var(--destructive));
          flex-shrink: 0;
        }
      }

      &.warning {
        background: hsla(var(--warning, 48 96% 53%), 0.1);
        border: 1px solid hsla(var(--warning, 48 96% 53%), 0.3);

        svg {
          color: hsl(var(--warning, 48 96% 53%));
          flex-shrink: 0;
        }
      }

      .result-content {
        flex: 1;
        font-size: var(--rf-font-size-sm);

        strong {
          display: block;
          margin-bottom: var(--rf-spacing-xs);
        }

        p {
          margin: 0;
          color: var(--rf-color-text-secondary);
        }
      }
    }

    .stats-section {
      margin-top: var(--rf-spacing-md);
      padding: var(--rf-spacing-md);
      background: var(--rf-color-bg-secondary);
      border-radius: var(--rf-radius-small);

      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--rf-spacing-sm);

      svg {
        color: var(--rf-color-text-secondary);
      }

      .stats-title {
        font-size: var(--rf-font-size-sm);
        font-weight: var(--rf-font-weight-semibold);
        color: var(--rf-color-text-secondary);
        margin-right: var(--rf-spacing-md);
      }

      .stats-grid {
        display: flex;
        flex-wrap: wrap;
        gap: var(--rf-spacing-lg);
      }

      .stat-item {
        display: flex;
        flex-direction: column;
        gap: var(--rf-spacing-xs);

        .stat-label {
          font-size: var(--rf-font-size-xs);
          color: var(--rf-color-text-secondary);
        }

        .stat-value {
          font-size: var(--rf-font-size-sm);
          font-weight: var(--rf-font-weight-semibold);
          color: var(--rf-color-text-primary);
        }
      }
    }
  }
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

// Dark mode adjustments
html.dark {
  .task-execution-panel {
    .output-terminal {
      background: #1a1a1a;
    }
  }
}
</style>
