<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2, RefreshCcw } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import BackgroundAgentPanel from '@/components/background-agent/BackgroundAgentPanel.vue'
import { getBackgroundAgent } from '@/api/background-agents'
import { listExecutionSessions } from '@/api/execution-console'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ThreadSelection } from '@/components/chat/threadItems'

const props = withDefaults(
  defineProps<{
    taskId: string
    selectedRunId?: string | null
  }>(),
  {
    selectedRunId: null,
  },
)

const emit = defineEmits<{
  refresh: []
  selectRun: [runId: string | null]
  selectThreadItem: [selection: ThreadSelection]
}>()

const { t } = useI18n()

const agent = ref<BackgroundAgent | null>(null)
const isLoading = ref(false)
const loadError = ref<string | null>(null)
const runOptions = ref<ExecutionSessionSummary[]>([])
const isLoadingRuns = ref(false)
const effectiveSelectedRunId = ref<string | null>(null)

const normalizedTaskId = computed(() => props.taskId.trim())

async function loadAgent() {
  if (!normalizedTaskId.value) {
    agent.value = null
    loadError.value = 'Missing task id'
    return
  }

  isLoading.value = true
  loadError.value = null

  try {
    agent.value = await getBackgroundAgent(normalizedTaskId.value)
  } catch (error) {
    agent.value = null
    loadError.value = error instanceof Error ? error.message : 'Failed to load background agent'
  } finally {
    isLoading.value = false
  }
}

async function loadRunOptions() {
  if (!normalizedTaskId.value) {
    runOptions.value = []
    effectiveSelectedRunId.value = null
    return
  }

  isLoadingRuns.value = true

  try {
    const sessions = await listExecutionSessions({
      container: {
        kind: 'background_task',
        id: normalizedTaskId.value,
      },
    })
    const sortedSessions = [...sessions].sort((left, right) => right.updated_at - left.updated_at)
    runOptions.value = sortedSessions

    const preferredRunId = props.selectedRunId
    const fallbackRunId = sortedSessions[0]?.run_id ?? null
    const resolvedRunId =
      preferredRunId && sortedSessions.some((session) => session.run_id === preferredRunId)
        ? preferredRunId
        : fallbackRunId

    effectiveSelectedRunId.value = resolvedRunId

    if (resolvedRunId !== preferredRunId) {
      emit('selectRun', resolvedRunId)
    }
  } catch (error) {
    runOptions.value = []
    effectiveSelectedRunId.value = null
    console.warn('Failed to load run options:', error)
  } finally {
    isLoadingRuns.value = false
  }
}

async function refreshPanel() {
  await Promise.all([loadAgent(), loadRunOptions()])
  emit('refresh')
}

async function handleSelectRun(event: Event) {
  const target = event.target as HTMLSelectElement
  const nextRunId = target.value || null
  effectiveSelectedRunId.value = nextRunId
  emit('selectRun', nextRunId)
}

function handleSelectThreadItem(selection: ThreadSelection) {
  emit('selectThreadItem', selection)
}

watch(
  () => props.selectedRunId,
  (value) => {
    effectiveSelectedRunId.value = value ?? null
  },
)

watch(
  () => props.taskId,
  () => {
    void loadAgent()
    void loadRunOptions()
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col" data-testid="workspace-run-panel">
    <div class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
      <div class="min-w-0">
        <h1 class="text-lg font-semibold text-foreground">
          {{ agent?.name ?? t('backgroundAgent.runTraceTitle') }}
        </h1>
        <p class="text-sm text-muted-foreground">
          {{ agent?.description || t('backgroundAgent.runTraceDescription') }}
        </p>
      </div>
      <div class="flex items-center gap-2">
        <select
          v-if="runOptions.length > 0"
          data-testid="workspace-run-selector"
          :value="effectiveSelectedRunId ?? ''"
          :disabled="isLoadingRuns"
          class="h-9 rounded-md border border-border bg-background px-3 text-sm"
          @change="handleSelectRun"
        >
          <option v-for="run in runOptions" :key="run.id" :value="run.run_id ?? ''">
            {{ run.title }}
          </option>
        </select>
        <Button variant="outline" size="sm" class="gap-2" @click="refreshPanel">
          <Loader2 v-if="isLoading || isLoadingRuns" :size="14" class="animate-spin" />
          <RefreshCcw v-else :size="14" />
          <span>{{ t('settings.marketplace.refresh') }}</span>
        </Button>
      </div>
    </div>

    <div
      v-if="isLoading && !agent"
      class="flex flex-1 items-center justify-center text-sm text-muted-foreground"
      data-testid="workspace-run-loading"
    >
      {{ t('backgroundAgent.loadingRun') }}
    </div>

    <div
      v-else-if="loadError"
      class="flex flex-1 items-center justify-center px-6 text-center text-sm text-destructive"
      data-testid="workspace-run-error"
    >
      {{ loadError }}
    </div>

    <div
      v-else-if="!agent"
      class="flex flex-1 items-center justify-center px-6 text-center text-sm text-muted-foreground"
      data-testid="workspace-run-not-found"
    >
      {{ t('backgroundAgent.runNotFound') }}
    </div>

    <BackgroundAgentPanel
      v-else
      :agent="agent"
      :selected-run-id="effectiveSelectedRunId"
      class="min-h-0 flex-1"
      @refresh="refreshPanel"
      @select-thread-item="handleSelectThreadItem"
    />
  </div>
</template>
