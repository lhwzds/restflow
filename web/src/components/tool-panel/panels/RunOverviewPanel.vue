<script setup lang="ts">
import { computed, ref } from 'vue'
import { GitBranch, ChevronDown, ChevronRight } from 'lucide-vue-next'
import type { ExecutionSessionSummary } from '@/types/generated/ExecutionSessionSummary'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'

const props = defineProps<{
  thread: ExecutionThread
  childRuns?: ExecutionSessionSummary[]
}>()

const emit = defineEmits<{
  navigateRun: [payload: { containerId: string; runId: string }]
}>()

const focus = computed(() => props.thread.focus)
const stats = computed(() => props.thread.timeline.stats)
const childRuns = computed(() => props.childRuns ?? [])
const showIds = ref(false)

function formatRelativeTime(value: bigint | number | null | undefined): string {
  if (value == null) return 'N/A'
  const numeric = typeof value === 'bigint' ? Number(value) : value
  if (!Number.isFinite(numeric)) return 'N/A'
  const diff = Date.now() - numeric
  if (diff < 60_000) return 'just now'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`
  return new Date(numeric).toLocaleDateString()
}

function formatCount(value: bigint | number | null | undefined): string {
  if (value == null) return '0'
  return String(value)
}
</script>

<template>
  <div class="space-y-4" data-testid="run-overview-panel">
    <div class="space-y-2">
      <div class="flex items-center gap-2">
        <span
          class="rounded bg-muted px-2 py-1 text-[10px] uppercase tracking-wide text-muted-foreground"
        >
          {{ focus.kind }}
        </span>
        <span
          class="rounded bg-muted px-2 py-1 text-[10px] uppercase tracking-wide text-muted-foreground"
          data-testid="run-overview-status"
        >
          {{ focus.status }}
        </span>
      </div>
      <div class="space-y-1">
        <p class="text-sm font-semibold" data-testid="run-overview-title">{{ focus.title }}</p>
        <p v-if="focus.subtitle" class="text-xs text-muted-foreground">
          {{ focus.subtitle }}
        </p>
      </div>
    </div>

    <!-- Primary stats: execution activity -->
    <div class="grid grid-cols-2 gap-2 text-xs">
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Messages</p>
        <p class="mt-1 font-medium">{{ formatCount(stats.message_count) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Tool calls</p>
        <p class="mt-1 font-medium">{{ formatCount(stats.tool_call_count) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">LLM calls</p>
        <p class="mt-1 font-medium">{{ formatCount(stats.llm_call_count) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Tokens</p>
        <p class="mt-1 font-medium">{{ formatCount(stats.total_tokens) }}</p>
      </div>
    </div>

    <!-- Secondary stats: timing and hierarchy -->
    <div class="grid grid-cols-2 gap-2 text-xs">
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Started</p>
        <p class="mt-1 font-medium">{{ formatRelativeTime(focus.started_at) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Updated</p>
        <p class="mt-1 font-medium">{{ formatRelativeTime(focus.updated_at) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Events</p>
        <p class="mt-1 font-medium" data-testid="run-overview-events">{{ formatCount(stats.total_events) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Child runs</p>
        <p class="mt-1 font-medium" data-testid="run-overview-child-runs">{{ childRuns.length }}</p>
      </div>
    </div>

    <!-- Model info -->
    <div class="flex items-center justify-between rounded-md border border-border bg-muted/10 px-3 py-2 text-xs">
      <span class="text-muted-foreground">Model</span>
      <span class="text-right">
        <span class="font-medium">{{ focus.effective_model || 'N/A' }}</span>
        <span v-if="focus.provider" class="ml-1 text-muted-foreground">· {{ focus.provider }}</span>
      </span>
    </div>

    <!-- IDs section — collapsed by default -->
    <div class="rounded-md border border-border bg-muted/10 text-xs">
      <button
        class="flex w-full items-center justify-between px-3 py-2 text-muted-foreground hover:text-foreground transition-colors"
        @click="showIds = !showIds"
      >
        <span class="font-medium">Run IDs</span>
        <ChevronDown v-if="showIds" :size="12" />
        <ChevronRight v-else :size="12" />
      </button>
      <div v-if="showIds" class="space-y-2 border-t border-border px-3 py-2">
        <div class="flex items-start justify-between gap-3">
          <span class="shrink-0 text-muted-foreground">Run</span>
          <span class="break-all text-right font-mono">{{ focus.run_id || 'N/A' }}</span>
        </div>
        <div class="flex items-start justify-between gap-3">
          <span class="shrink-0 text-muted-foreground">Session</span>
          <span class="break-all text-right font-mono">{{ focus.session_id || 'N/A' }}</span>
        </div>
        <div class="flex items-start justify-between gap-3">
          <span class="shrink-0 text-muted-foreground">Root</span>
          <span class="break-all text-right font-mono">{{ focus.root_run_id || 'N/A' }}</span>
        </div>
        <div v-if="focus.parent_run_id" class="flex items-start justify-between gap-3">
          <span class="shrink-0 text-muted-foreground">Parent</span>
          <span class="break-all text-right font-mono">{{ focus.parent_run_id }}</span>
        </div>
      </div>
    </div>

    <div
      v-if="childRuns.length > 0"
      class="space-y-2 rounded-md border border-border bg-muted/10 p-3"
      data-testid="run-overview-child-run-list"
    >
      <div class="flex items-center gap-2">
        <GitBranch :size="14" class="text-muted-foreground" />
        <p class="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Direct child runs
        </p>
      </div>
      <div class="space-y-2">
        <button
          v-for="childRun in childRuns"
          :key="childRun.id"
          :data-testid="`run-overview-child-run-${childRun.run_id ?? childRun.id}`"
          class="flex w-full items-start justify-between gap-3 rounded-md border border-border/60 bg-background/80 px-3 py-2 text-left transition-colors hover:bg-muted/60"
          @click="
            childRun.run_id &&
              emit('navigateRun', {
                containerId: childRun.container_id,
                runId: childRun.run_id,
              })
          "
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm font-medium">{{ childRun.title }}</p>
            <p class="truncate text-xs text-muted-foreground">
              {{ childRun.agent_id || 'Unknown agent' }}
            </p>
          </div>
          <div class="shrink-0 text-right text-[11px] text-muted-foreground">
            <p>{{ childRun.status }}</p>
            <p>{{ formatRelativeTime(childRun.updated_at) }}</p>
          </div>
        </button>
      </div>
    </div>
  </div>
</template>
