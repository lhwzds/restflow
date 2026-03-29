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

type DiffLineType = 'added' | 'removed' | 'unchanged'

interface DiffLine {
  type: DiffLineType
  text: string
}

// LCS-based line diff
function computeDiff(oldText: string, newText: string): DiffLine[] {
  const oldLines = oldText.split('\n')
  const newLines = newText.split('\n')
  const m = oldLines.length
  const n = newLines.length

  // Flat 1D table: dp[i * (n+1) + j] = LCS length of oldLines[0..i-1] and newLines[0..j-1]
  const size = (m + 1) * (n + 1)
  const dp = new Int32Array(size)
  const idx = (r: number, c: number): number => r * (n + 1) + c

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[idx(i, j)] =
        oldLines[i - 1] === newLines[j - 1]
          ? (dp[idx(i - 1, j - 1)] ?? 0) + 1
          : Math.max(dp[idx(i - 1, j)] ?? 0, dp[idx(i, j - 1)] ?? 0)
    }
  }

  const result: DiffLine[] = []
  let i = m
  let j = n
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
      result.unshift({ type: 'unchanged', text: oldLines[i - 1] ?? '' })
      i--
      j--
    } else if (j > 0 && (i === 0 || (dp[idx(i, j - 1)] ?? 0) >= (dp[idx(i - 1, j)] ?? 0))) {
      result.unshift({ type: 'added', text: newLines[j - 1] ?? '' })
      j--
    } else {
      result.unshift({ type: 'removed', text: oldLines[i - 1] ?? '' })
      i--
    }
  }
  return result
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

// Diff view data for write/edit actions
const writeContent = computed(() =>
  typeof args.value.content === 'string' ? args.value.content : '',
)
const oldString = computed(() =>
  typeof args.value.old_string === 'string' ? args.value.old_string : '',
)
const newString = computed(() =>
  typeof args.value.new_string === 'string' ? args.value.new_string : '',
)

const diffLines = computed<DiffLine[]>(() => {
  if (action.value === 'write' && writeContent.value) {
    return writeContent.value.split('\n').map((line) => ({ type: 'added' as const, text: line }))
  }
  if (action.value === 'edit' && (oldString.value || newString.value)) {
    return computeDiff(oldString.value, newString.value)
  }
  return []
})

const diffStats = computed(() => {
  const added = diffLines.value.filter((l) => l.type === 'added').length
  const removed = diffLines.value.filter((l) => l.type === 'removed').length
  return { added, removed }
})

const showDiff = computed(() =>
  (action.value === 'write' || action.value === 'edit') && diffLines.value.length > 0,
)

const diffCopyText = computed(() => {
  if (action.value === 'write') return writeContent.value
  return newString.value
})

async function onCopyContent(): Promise<void> {
  await copyText(showDiff.value ? diffCopyText.value : contentText.value)
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
      <!-- Diff view for write / edit actions -->
      <div v-if="showDiff" data-testid="file-diff-view">
        <div class="mb-2 flex items-center gap-3 text-[11px]">
          <span v-if="diffStats.added > 0" class="text-emerald-600">+{{ diffStats.added }}</span>
          <span v-if="diffStats.removed > 0" class="text-rose-600">−{{ diffStats.removed }}</span>
        </div>
        <div class="overflow-hidden rounded border border-border font-mono text-xs">
          <div
            v-for="(line, index) in diffLines"
            :key="index"
            class="grid grid-cols-[1.5rem_1fr] border-b border-border/40 last:border-b-0"
            :class="{
              'bg-emerald-50 text-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200': line.type === 'added',
              'bg-rose-50 text-rose-900 dark:bg-rose-950/40 dark:text-rose-200': line.type === 'removed',
            }"
          >
            <span
              class="select-none px-1 text-center"
              :class="{
                'text-emerald-500': line.type === 'added',
                'text-rose-500': line.type === 'removed',
                'text-muted-foreground/40': line.type === 'unchanged',
              }"
            >{{ line.type === 'added' ? '+' : line.type === 'removed' ? '−' : ' ' }}</span>
            <span class="overflow-x-auto whitespace-pre px-2 py-0.5">{{ line.text }}</span>
          </div>
        </div>
      </div>

      <!-- Read: line-numbered content -->
      <div
        v-else-if="action === 'read' && contentText"
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
