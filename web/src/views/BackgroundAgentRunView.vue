<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, RefreshCcw, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import BackgroundAgentPanel from '@/components/background-agent/BackgroundAgentPanel.vue'
import { getBackgroundAgent } from '@/api/background-agents'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()

const agent = ref<BackgroundAgent | null>(null)
const isLoading = ref(false)
const loadError = ref<string | null>(null)

const taskId = computed(() => String(route.params.taskId ?? ''))

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

function goBack() {
  void router.push({ name: 'workspace' })
}

watch(
  taskId,
  () => {
    void loadAgent()
  },
  { immediate: true },
)
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
      <Button variant="outline" size="sm" class="gap-2" @click="loadAgent">
        <Loader2 v-if="isLoading" :size="14" class="animate-spin" />
        <RefreshCcw v-else :size="14" />
        <span>{{ t('settings.marketplace.refresh') }}</span>
      </Button>
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

    <BackgroundAgentPanel v-else :agent="agent" class="min-h-0 flex-1" @refresh="loadAgent" />
  </div>
</template>
