<script setup lang="ts">
import { computed, ref } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import { copyText, parseToolResult } from '@/components/tool-panel/utils'

const props = defineProps<{
  step: StreamStep
}>()

const rawMode = ref(false)
const parsed = computed(() => parseToolResult(props.step))
const formatted = computed(() => JSON.stringify(parsed.value, null, 2))
const raw = computed(() => (props.step.result ?? '').toString())
const content = computed(() => (rawMode.value ? raw.value : formatted.value))

function statusClass(): string {
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
        <span class="font-medium">{{ props.step.name }}</span>
        <span class="rounded px-2 py-0.5" :class="statusClass()">{{ props.step.status }}</span>
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
