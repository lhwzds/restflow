<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, RefreshCcw, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import BackgroundAgentPanel from '@/components/background-agent/BackgroundAgentPanel.vue'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'
import { getBackgroundAgent } from '@/api/background-agents'
import { listExecutionSessions } from '@/api/execution-console'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ThreadSelection } from '@/components/chat/threadItems'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const toolPanel = useToolPanel()

const agent = ref<BackgroundAgent | null>(null)
const isLoading = ref(false)
const loadError = ref<string | null>(null)
const runOptions = ref<ExecutionSessionSummary[]>([])
const isLoadingRuns = ref(false)

const taskId = computed(() => String(route.params.taskId ?? ''))
const routeRunId = computed(() => {
  const value = route.query.runId
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null
})
const selectedRunId = ref<string | null>(null)

async function loadAgent() {
  const currentTaskId = taskId.value.trim()
  if (!currentTaskId) {
    agent.value = null
    loadError.value = 'Missing task id'
    return
  }

  isLoading.value = true
  loadError.value = null
  try {
    agent.value = await getBackgroundAgent(currentTaskId)
  } catch (error) {
    agent.value = null
    loadError.value = error instanceof Error ? error.message : 'Failed to load background agent'
  } finally {
    isLoading.value = false
  }
}

async function loadRunOptions() {
  const currentTaskId = taskId.value.trim()
  if (!currentTaskId) {
    runOptions.value = []
    selectedRunId.value = null
    return
  }

  isLoadingRuns.value = true
  try {
    const sessions = await listExecutionSessions({
      container: {
        kind: 'background_task',
        id: currentTaskId,
      },
    })
    const sortedSessions = [...sessions].sort((left, right) => right.updated_at - left.updated_at)
    runOptions.value = sortedSessions

    const preferredRunId = routeRunId.value
    const fallbackRunId = sortedSessions[0]?.run_id ?? null
    const resolvedRunId =
      preferredRunId && sortedSessions.some((session) => session.run_id === preferredRunId)
        ? preferredRunId
        : fallbackRunId

    selectedRunId.value = resolvedRunId

    if (resolvedRunId !== preferredRunId) {
      await router.replace({
        name: 'workspace-run',
        params: { taskId: currentTaskId },
        query: resolvedRunId ? { runId: resolvedRunId } : undefined,
      })
    }
  } catch (error) {
    runOptions.value = []
    selectedRunId.value = null
    console.warn('Failed to load run options:', error)
  } finally {
    isLoadingRuns.value = false
  }
}

async function handleSelectRun(event: Event) {
  const target = event.target as HTMLSelectElement
  const nextRunId = target.value || null
  selectedRunId.value = nextRunId
  await router.replace({
    name: 'workspace-run',
    params: { taskId: taskId.value },
    query: nextRunId ? { runId: nextRunId } : undefined,
  })
}

function goBack() {
  void router.push({ name: 'workspace' })
}

function handleSelectThreadItem(selection: ThreadSelection) {
  toolPanel.handleThreadSelection(selection)
}

watch(
  taskId,
  () => {
    void loadAgent()
    void loadRunOptions()
  },
  { immediate: true },
)

watch(routeRunId, (value) => {
  selectedRunId.value = value
})
</script>

<template>
  <div class="flex h-screen flex-col bg-background" data-testid="background-agent-run-view">
    <div class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <Button variant="ghost" size="sm" class="gap-1 px-2" @click="goBack">
            <ArrowLeft :size="14" />
            <span>{{ t('backgroundAgent.backToWorkspace') }}</span>
          </Button>
        </div>
        <h1 class="mt-2 text-lg font-semibold text-foreground">
          {{ agent?.name ?? t('backgroundAgent.runTraceTitle') }}
        </h1>
        <p class="text-sm text-muted-foreground">
          {{ t('backgroundAgent.runTraceDescription') }}
        </p>
      </div>
      <div class="flex items-center gap-2">
        <select
          v-if="runOptions.length > 0"
          data-testid="background-run-selector"
          :value="selectedRunId ?? ''"
          :disabled="isLoadingRuns"
          class="h-9 rounded-md border border-border bg-background px-3 text-sm"
          @change="handleSelectRun"
        >
          <option
            v-for="run in runOptions"
            :key="run.id"
            :value="run.run_id ?? ''"
          >
            {{ run.title }}
          </option>
        </select>
        <Button
          variant="outline"
          size="sm"
          class="gap-2"
          @click="
            () => {
              void loadAgent()
              void loadRunOptions()
            }
          "
        >
        <Loader2 v-if="isLoading || isLoadingRuns" :size="14" class="animate-spin" />
        <RefreshCcw v-else :size="14" />
        <span>{{ t('settings.marketplace.refresh') }}</span>
        </Button>
      </div>
    </div>

    <div v-if="isLoading && !agent" class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
      {{ t('backgroundAgent.loadingRun') }}
    </div>

    <div
      v-else-if="loadError"
      class="flex flex-1 items-center justify-center px-6 text-center text-sm text-destructive"
    >
      {{ loadError }}
    </div>

    <div
      v-else-if="!agent"
      class="flex flex-1 items-center justify-center px-6 text-center text-sm text-muted-foreground"
    >
      {{ t('backgroundAgent.runNotFound') }}
    </div>

    <div v-else class="flex min-h-0 flex-1">
      <BackgroundAgentPanel
        :agent="agent"
        :selected-run-id="selectedRunId"
        class="min-h-0 flex-1"
        @refresh="
          () => {
            void loadAgent()
            void loadRunOptions()
          }
        "
        @select-thread-item="handleSelectThreadItem"
      />

      <ToolPanel
        v-if="toolPanel.visible.value && toolPanel.activeEntry.value"
        :panel-type="toolPanel.state.value.panelType"
        :title="toolPanel.state.value.title"
        :tool-name="toolPanel.state.value.toolName"
        :data="toolPanel.state.value.data"
        :step="toolPanel.state.value.step"
        :can-navigate-prev="toolPanel.canNavigatePrev.value"
        :can-navigate-next="toolPanel.canNavigateNext.value"
        @navigate="toolPanel.navigateHistory"
        @close="toolPanel.closePanel()"
      />
    </div>
  </div>
</template>
