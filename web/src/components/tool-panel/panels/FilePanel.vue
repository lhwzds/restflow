<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import {
  copyText,
  detectLanguage,
  parseToolArguments,
  parseToolResult,
} from '@/components/tool-panel/utils'

interface FileMatch {
  path?: string
  line?: number
  text?: string
}

interface FileEntry {
  name?: string
  type?: string
  size?: number
}

interface FileResult {
  path?: string
  content?: string
  lines?: number
  action?: string
  written?: boolean
  bytes?: number
  matches?: FileMatch[]
  entries?: FileEntry[]
}

const props = defineProps<{
  step: StreamStep
}>()

const args = computed(() => parseToolArguments(props.step))
const result = computed<FileResult>(() => {
  const parsed = parseToolResult(props.step)
  return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    ? (parsed as FileResult)
    : {}
})

const action = computed(() => {
  const raw = result.value.action ?? args.value.action
  return typeof raw === 'string' ? raw : 'unknown'
})
const filePath = computed(() => {
  const raw = result.value.path ?? args.value.path
  return typeof raw === 'string' ? raw : ''
})
const language = computed(() => detectLanguage(filePath.value))
const contentText = computed(() =>
  typeof result.value.content === 'string' ? result.value.content : '',
)
const contentLines = computed(() => contentText.value.split('\n'))
const matches = computed(() => (Array.isArray(result.value.matches) ? result.value.matches : []))
const entries = computed(() => (Array.isArray(result.value.entries) ? result.value.entries : []))

async function onCopyContent(): Promise<void> {
  await copyText(contentText.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="file-panel">
    <header class="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
      <div class="min-w-0 space-y-1">
        <p class="truncate text-xs font-medium">{{ filePath || 'file tool' }}</p>
        <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
          <span class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">{{ action }}</span>
          <span class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">{{ language }}</span>
        </div>
      </div>
      <button
        class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
        @click="onCopyContent"
      >
        Copy
      </button>
    </header>

    <div class="space-y-3 p-3">
      <div
        v-if="action === 'read' && contentText"
        class="overflow-hidden rounded border border-border"
      >
        <div
          v-for="(line, index) in contentLines"
          :key="index"
          class="grid grid-cols-[3rem_1fr] border-b border-border/60 px-2 py-0.5 font-mono text-xs last:border-b-0"
        >
          <span class="text-muted-foreground">{{ index + 1 }}</span>
          <span class="overflow-x-auto">{{ line }}</span>
        </div>
      </div>

      <div v-else-if="action === 'search' && matches.length" class="space-y-2">
        <article
          v-for="(match, index) in matches"
          :key="index"
          class="rounded border border-border p-2 text-xs"
        >
          <p class="font-medium">
            {{ match.path || filePath || 'unknown path' }}:{{ match.line ?? '?' }}
          </p>
          <p class="mt-1 font-mono text-muted-foreground">{{ match.text || '' }}</p>
        </article>
      </div>

      <ul v-else-if="action === 'list' && entries.length" class="space-y-1 text-xs">
        <li
          v-for="(entry, index) in entries"
          :key="index"
          class="flex items-center justify-between rounded border border-border px-2 py-1"
        >
          <span>{{ entry.name || 'unknown' }}</span>
          <span class="text-muted-foreground"
            >{{ entry.type || 'entry' }} {{ entry.size ?? '-' }}</span
          >
        </li>
      </ul>

      <div v-else class="rounded bg-muted/40 p-2 text-xs">
        <p v-if="result.written !== undefined">written: {{ result.written ? 'true' : 'false' }}</p>
        <p v-if="result.bytes !== undefined">bytes: {{ result.bytes }}</p>
        <pre v-if="!result.written && !result.bytes" class="font-mono">{{
          JSON.stringify(result, null, 2)
        }}</pre>
      </div>
    </div>
  </section>
</template>
