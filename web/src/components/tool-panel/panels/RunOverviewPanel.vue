<script setup lang="ts">
import { computed } from 'vue'
import type { ExecutionThread } from '@/types/generated/ExecutionThread'

const props = defineProps<{
  thread: ExecutionThread
}>()

const focus = computed(() => props.thread.focus)
const stats = computed(() => props.thread.timeline.stats)

function formatTimestamp(value: bigint | number | null | undefined): string {
  if (value == null) return 'N/A'
  const numeric = typeof value === 'bigint' ? Number(value) : value
  if (!Number.isFinite(numeric)) return 'N/A'
  return new Date(numeric).toLocaleString()
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

    <div class="grid grid-cols-2 gap-2 text-xs">
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Events</p>
        <p class="mt-1 font-medium" data-testid="run-overview-events">
          {{ formatCount(stats.total_events) }}
        </p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Child runs</p>
        <p class="mt-1 font-medium" data-testid="run-overview-child-runs">
          {{ props.thread.child_sessions.length }}
        </p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Started</p>
        <p class="mt-1 font-medium">{{ formatTimestamp(focus.started_at) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Updated</p>
        <p class="mt-1 font-medium">{{ formatTimestamp(focus.updated_at) }}</p>
      </div>
    </div>

    <div class="space-y-2 rounded-md border border-border bg-muted/10 p-3 text-xs">
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Run ID</span>
        <span class="break-all text-right font-mono">{{ focus.run_id || 'N/A' }}</span>
      </div>
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Session ID</span>
        <span class="break-all text-right font-mono">{{ focus.session_id || 'N/A' }}</span>
      </div>
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Root run</span>
        <span class="break-all text-right font-mono">{{ focus.root_run_id || 'N/A' }}</span>
      </div>
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Parent run</span>
        <span class="break-all text-right font-mono">{{ focus.parent_run_id || 'N/A' }}</span>
      </div>
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Model</span>
        <span class="break-all text-right">{{ focus.effective_model || 'N/A' }}</span>
      </div>
      <div class="flex items-start justify-between gap-3">
        <span class="text-muted-foreground">Provider</span>
        <span class="break-all text-right">{{ focus.provider || 'N/A' }}</span>
      </div>
    </div>

    <div class="grid grid-cols-2 gap-2 text-xs">
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Messages</p>
        <p class="mt-1 font-medium">{{ formatCount(stats.message_count) }}</p>
      </div>
      <div class="rounded-md border border-border bg-muted/20 p-2">
        <p class="text-muted-foreground">Tools</p>
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
  </div>
</template>
