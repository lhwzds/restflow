<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const props = defineProps<{
  isActive: boolean
  startedAt: number | null
  steps: StreamStep[]
  fallbackLabel?: string | null
}>()

const elapsedSeconds = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

function startTimer() {
  if (timer) clearInterval(timer)
  elapsedSeconds.value = props.startedAt ? Math.floor((Date.now() - props.startedAt) / 1000) : 0
  timer = setInterval(() => {
    elapsedSeconds.value = props.startedAt ? Math.floor((Date.now() - props.startedAt) / 1000) : 0
  }, 1000)
}

function stopTimer() {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

watch(
  () => props.isActive,
  (active) => {
    if (active) {
      startTimer()
    } else {
      stopTimer()
    }
  },
  { immediate: true },
)

onUnmounted(stopTimer)

const currentStep = computed(() => props.steps.find((s) => s.status === 'running'))
const completedCount = computed(() => props.steps.filter((s) => s.status === 'completed').length)
const failedCount = computed(() => props.steps.filter((s) => s.status === 'failed').length)
const displayLabel = computed(
  () => currentStep.value?.displayName ?? currentStep.value?.name ?? props.fallbackLabel ?? null,
)

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${m}m ${s}s`
}
</script>

<template>
  <div
    class="flex items-center gap-2 px-4 py-1.5 text-xs text-muted-foreground border-b border-border bg-muted/20 shrink-0"
    data-testid="execution-status-bar"
  >
    <span class="w-1.5 h-1.5 rounded-full bg-primary animate-pulse shrink-0" />
    <span class="tabular-nums font-mono">{{ formatElapsed(elapsedSeconds) }}</span>
    <template v-if="displayLabel">
      <span class="text-muted-foreground/50">·</span>
      <span class="truncate">{{ displayLabel }}</span>
    </template>
    <div class="ml-auto flex items-center gap-1.5 shrink-0">
      <span v-if="completedCount > 0">{{ completedCount }} done</span>
      <span v-if="completedCount > 0 && (currentStep || failedCount > 0)" class="text-muted-foreground/50">·</span>
      <span v-if="currentStep">1 running</span>
      <span v-if="failedCount > 0" class="text-destructive">{{ failedCount }} failed</span>
    </div>
  </div>
</template>
