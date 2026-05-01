<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import {
  copyText,
  formatDuration,
  parseToolArguments,
  parseToolResult,
} from '@/components/tool-panel/utils'

type JsonRecord = Record<string, unknown>

interface BrowserExecutionResult {
  runtime?: string
  exit_code?: number
  duration_ms?: number
  stderr?: string
  payload?: JsonRecord
}

const props = defineProps<{
  step: StreamStep
}>()

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as JsonRecord)
    : null
}

function getString(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

function toJson(value: unknown): string {
  if (typeof value === 'string') return value
  return JSON.stringify(value, null, 2)
}

const args = computed(() => parseToolArguments(props.step))
const result = computed<BrowserExecutionResult>(() => {
  const parsed = parseToolResult(props.step)
  const record = asRecord(parsed)
  return record ? (record as BrowserExecutionResult) : {}
})

const operation = computed(() => getString(args.value.action))
const sessionId = computed(() => getString(args.value.session_id))
const runtime = computed(() => getString(result.value.runtime))
const exitCode = computed(() =>
  typeof result.value.exit_code === 'number' ? result.value.exit_code : null,
)
const duration = computed(() =>
  typeof result.value.duration_ms === 'number' ? formatDuration(result.value.duration_ms) : '',
)
const stderrText = computed(() => getString(result.value.stderr))

const payload = computed<JsonRecord>(() => asRecord(result.value.payload) ?? {})
const payloadSuccess = computed(() =>
  typeof payload.value.success === 'boolean' ? payload.value.success : exitCode.value === 0,
)
const payloadError = computed(() => getString(payload.value.error))
const payloadResult = computed(() => payload.value.result)

const actionResults = computed<JsonRecord[]>(() => {
  if (!Array.isArray(payloadResult.value)) return []
  return payloadResult.value
    .map((item) => asRecord(item))
    .filter((item): item is JsonRecord => item !== null)
})

const navigateUrls = computed<string[]>(() =>
  actionResults.value
    .filter((item) => getString(item.type) === 'navigate')
    .map((item) => getString(item.url))
    .filter((url) => url.length > 0),
)

const screenshotPaths = computed<string[]>(() =>
  actionResults.value
    .filter((item) => getString(item.type) === 'screenshot')
    .map((item) => getString(item.path))
    .filter((path) => path.length > 0),
)

const scriptResultText = computed(() => {
  if (Array.isArray(payloadResult.value) || payloadResult.value === undefined) return ''
  return toJson(payloadResult.value)
})

const rawArguments = computed(() => JSON.stringify(args.value, null, 2))
const rawResult = computed(() => JSON.stringify(result.value, null, 2))

function actionLabel(item: JsonRecord): string {
  const actionType = getString(item.type)
  if (!actionType) return 'action'

  if (actionType === 'navigate') {
    const url = getString(item.url)
    return url ? `navigate: ${url}` : actionType
  }

  if (actionType === 'screenshot') {
    const path = getString(item.path)
    return path ? `screenshot: ${path}` : actionType
  }

  if (actionType === 'extract_text') {
    const selector = getString(item.selector)
    return selector ? `extract_text: ${selector}` : actionType
  }

  return actionType
}

async function onCopyRawResult(): Promise<void> {
  await copyText(rawResult.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="browser-panel">
    <header class="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
      <div class="flex items-center gap-2 text-xs">
        <span
          class="rounded px-2 py-0.5 font-medium"
          :class="payloadSuccess ? 'bg-emerald-100 text-emerald-800' : 'bg-rose-100 text-rose-800'"
        >
          {{ payloadSuccess ? 'success' : 'failed' }}
        </span>
        <span v-if="runtime" class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">
          {{ runtime }}
        </span>
        <span v-if="exitCode !== null" class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">
          exit {{ exitCode }}
        </span>
        <span v-if="duration" class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">
          {{ duration }}
        </span>
      </div>
      <button
        class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
        @click="onCopyRawResult"
      >
        Copy raw
      </button>
    </header>

    <div class="space-y-3 p-3 text-xs">
      <div class="grid grid-cols-2 gap-2">
        <div class="rounded bg-muted/40 px-2 py-1">
          <p class="text-muted-foreground">Action</p>
          <p class="font-mono">{{ operation || 'unknown' }}</p>
        </div>
        <div class="rounded bg-muted/40 px-2 py-1">
          <p class="text-muted-foreground">Session</p>
          <p class="font-mono truncate">{{ sessionId || 'n/a' }}</p>
        </div>
      </div>

      <details v-if="navigateUrls.length > 0" open class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer font-medium">Navigated URLs</summary>
        <ul class="mt-2 space-y-1">
          <li v-for="url in navigateUrls" :key="url" class="break-all font-mono">
            {{ url }}
          </li>
        </ul>
      </details>

      <details
        v-if="screenshotPaths.length > 0"
        open
        class="rounded border border-border px-2 py-1"
      >
        <summary class="cursor-pointer font-medium">Screenshots</summary>
        <ul class="mt-2 space-y-2">
          <li v-for="path in screenshotPaths" :key="path" class="space-y-1">
            <p class="break-all font-mono">{{ path }}</p>
            <img
              :src="path"
              alt="browser screenshot"
              class="max-h-48 rounded border border-border object-contain"
            />
          </li>
        </ul>
      </details>

      <details v-if="actionResults.length > 0" open class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer font-medium">Action Results</summary>
        <ul class="mt-2 space-y-2">
          <li
            v-for="(item, index) in actionResults"
            :key="`${index}-${actionLabel(item)}`"
            class="rounded bg-muted/30 p-2"
          >
            <p class="mb-1 font-medium">{{ actionLabel(item) }}</p>
            <pre class="overflow-x-auto font-mono text-xs"><code>{{ toJson(item) }}</code></pre>
          </li>
        </ul>
      </details>

      <details v-else-if="scriptResultText" open class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer font-medium">Script Result</summary>
        <pre
          class="mt-2 overflow-x-auto font-mono text-xs"
        ><code>{{ scriptResultText }}</code></pre>
      </details>

      <p
        v-if="stderrText || payloadError"
        class="rounded border border-rose-300 bg-rose-50 px-2 py-1 text-rose-800"
      >
        {{ payloadError || stderrText }}
      </p>

      <details class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer font-medium">Arguments</summary>
        <pre class="mt-2 overflow-x-auto font-mono text-xs"><code>{{ rawArguments }}</code></pre>
      </details>
    </div>
  </section>
</template>
