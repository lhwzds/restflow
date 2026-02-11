<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import {
  copyText,
  formatDuration,
  parseToolArguments,
  parseToolResult,
} from '@/components/tool-panel/utils'

interface TerminalResult {
  exit_code?: number
  stdout?: string
  stderr?: string
  truncated?: boolean
  duration_ms?: number
}

const props = defineProps<{
  step: StreamStep
}>()

const args = computed(() => parseToolArguments(props.step))
const result = computed<TerminalResult>(() => {
  const parsed = parseToolResult(props.step)
  return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    ? (parsed as TerminalResult)
    : {}
})

const command = computed(() => {
  const raw = args.value.command ?? args.value.cmd
  return typeof raw === 'string' ? raw : ''
})
const exitCode = computed(() =>
  typeof result.value.exit_code === 'number' ? result.value.exit_code : null,
)
const duration = computed(() =>
  typeof result.value.duration_ms === 'number' ? formatDuration(result.value.duration_ms) : null,
)
const stdoutText = computed(() =>
  typeof result.value.stdout === 'string' ? result.value.stdout : '',
)
const stderrText = computed(() =>
  typeof result.value.stderr === 'string' ? result.value.stderr : '',
)

function exitCodeClass(code: number | null): string {
  if (code === null) return 'bg-zinc-100 text-zinc-800'
  return code === 0 ? 'bg-emerald-100 text-emerald-800' : 'bg-rose-100 text-rose-800'
}

async function onCopyStdout(): Promise<void> {
  await copyText(stdoutText.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="terminal-panel">
    <header class="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
      <div class="flex items-center gap-2 text-xs">
        <span class="rounded px-2 py-0.5 font-medium" :class="exitCodeClass(exitCode)">
          exit {{ exitCode ?? 'n/a' }}
        </span>
        <span v-if="duration" class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">{{
          duration
        }}</span>
      </div>
      <button
        class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
        @click="onCopyStdout"
      >
        Copy
      </button>
    </header>

    <div class="space-y-3 p-3">
      <p v-if="command" class="rounded bg-muted/50 px-2 py-1 font-mono text-xs">$ {{ command }}</p>

      <div v-if="stdoutText" class="space-y-1">
        <p class="text-xs font-medium text-muted-foreground">stdout</p>
        <pre
          class="overflow-x-auto rounded bg-emerald-50 p-2 font-mono text-xs text-emerald-900"
        ><code>{{ stdoutText }}</code></pre>
      </div>

      <div v-if="stderrText" class="space-y-1">
        <p class="text-xs font-medium text-muted-foreground">stderr</p>
        <pre
          class="overflow-x-auto rounded bg-rose-50 p-2 font-mono text-xs text-rose-900"
        ><code>{{ stderrText }}</code></pre>
      </div>

      <p
        v-if="result.truncated === true"
        class="rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-800"
      >
        Output was truncated.
      </p>
    </div>
  </section>
</template>
