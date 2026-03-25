<script setup lang="ts">
import { computed, ref } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import { copyText, parseToolResult } from '@/components/tool-panel/utils'

const props = defineProps<{
  step?: StreamStep
  data?: Record<string, unknown>
}>()

const rawMode = ref(false)

function stringify(value: unknown, spacing: number): string {
  return JSON.stringify(
    value,
    (_key, current) => (typeof current === 'bigint' ? current.toString() : current),
    spacing,
  )
}

const parsed = computed(() => (props.step ? parseToolResult(props.step) : props.data ?? {}))
const formatted = computed(() => stringify(parsed.value, 2))
const raw = computed(() =>
  props.step ? (props.step.result ?? '').toString() : stringify(props.data ?? {}, 0),
)
const content = computed(() => (rawMode.value ? raw.value : formatted.value))
const hasStep = computed(() => !!props.step)

function statusClass(): string {
  if (!props.step) return 'bg-zinc-100 text-zinc-800'
  switch (props.step.status) {
    case 'completed':
      return 'bg-emerald-100 text-emerald-800'
    case 'failed':
      return 'bg-rose-100 text-rose-800'
    case 'running':
      return 'bg-sky-100 text-sky-800'
    default:
      return 'bg-zinc-100 text-zinc-800'
  }
}

async function onCopy(): Promise<void> {
  await copyText(content.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="generic-json-panel">
    <header class="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
      <div class="flex items-center gap-2 text-xs">
        <span class="font-medium">{{ props.step?.name ?? 'Details' }}</span>
        <span v-if="hasStep" class="rounded px-2 py-0.5" :class="statusClass()">{{
          props.step?.status
        }}</span>
      </div>
      <div class="flex items-center gap-2">
        <button
          class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
          @click="rawMode = !rawMode"
        >
          {{ rawMode ? 'Formatted' : 'Raw' }}
        </button>
        <button
          class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
          @click="onCopy"
        >
          Copy
        </button>
      </div>
    </header>
    <pre class="overflow-x-auto p-3 font-mono text-xs"><code>{{ content }}</code></pre>
  </section>
</template>
