<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import {
  copyText,
  methodColor,
  parseToolArguments,
  parseToolResult,
  statusColor,
} from '@/components/tool-panel/utils'

interface HttpResult {
  status?: number
  body?: unknown
}

const props = defineProps<{
  step: StreamStep
}>()

const args = computed(() => parseToolArguments(props.step))
const result = computed<HttpResult>(() => {
  const parsed = parseToolResult(props.step)
  return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    ? (parsed as HttpResult)
    : {}
})

const method = computed(() => {
  const raw = args.value.method
  return typeof raw === 'string' ? raw.toUpperCase() : 'GET'
})

const url = computed(() => {
  const raw = args.value.url
  return typeof raw === 'string' ? raw : ''
})

const status = computed(() =>
  typeof result.value.status === 'number' ? result.value.status : null,
)
const requestHeaders = computed(() => {
  const raw = args.value.headers
  if (typeof raw === 'string') return raw
  if (typeof raw === 'object' && raw !== null) return JSON.stringify(raw, null, 2)
  return ''
})
const requestBody = computed(() => {
  const raw = args.value.body
  if (typeof raw === 'string') return raw
  if (raw !== undefined) return JSON.stringify(raw, null, 2)
  return ''
})
const responseBody = computed(() => {
  if (typeof result.value.body === 'string') return result.value.body
  if (result.value.body !== undefined) return JSON.stringify(result.value.body, null, 2)
  return ''
})

async function onCopyResponse(): Promise<void> {
  await copyText(responseBody.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="http-panel">
    <header class="space-y-2 border-b border-border px-3 py-2">
      <div class="flex items-center gap-2 text-xs">
        <span class="rounded px-2 py-0.5 font-medium" :class="methodColor(method)">{{
          method
        }}</span>
        <span
          v-if="status !== null"
          class="rounded px-2 py-0.5 font-medium"
          :class="statusColor(status)"
        >
          {{ status }}
        </span>
      </div>
      <p v-if="url" class="truncate font-mono text-xs">{{ url }}</p>
    </header>

    <div class="space-y-3 p-3">
      <details v-if="requestHeaders" open class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer text-xs font-medium">Request headers</summary>
        <pre class="mt-2 overflow-x-auto font-mono text-xs"><code>{{ requestHeaders }}</code></pre>
      </details>

      <details v-if="requestBody" open class="rounded border border-border px-2 py-1">
        <summary class="cursor-pointer text-xs font-medium">Request body</summary>
        <pre class="mt-2 overflow-x-auto font-mono text-xs"><code>{{ requestBody }}</code></pre>
      </details>

      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <p class="text-xs font-medium text-muted-foreground">Response</p>
          <button
            class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
            @click="onCopyResponse"
          >
            Copy
          </button>
        </div>
        <pre
          class="overflow-x-auto rounded bg-muted/50 p-2 font-mono text-xs"
        ><code>{{ responseBody || '(empty)' }}</code></pre>
      </div>
    </div>
  </section>
</template>
