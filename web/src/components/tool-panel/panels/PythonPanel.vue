<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import { copyText, parseToolArguments, parseToolResult } from '@/components/tool-panel/utils'

interface PythonResult {
  stdout?: string
  stderr?: string
  exit_code?: number
  timed_out?: boolean
  runtime?: string
}

const props = defineProps<{
  step: StreamStep
}>()

const args = computed(() => parseToolArguments(props.step))
const result = computed<PythonResult>(() => {
  const parsed = parseToolResult(props.step)
  return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    ? (parsed as PythonResult)
    : {}
})

const code = computed(() => {
  const raw = args.value.code
  return typeof raw === 'string' ? raw : ''
})
const runtime = computed(() => {
  const raw = result.value.runtime ?? args.value.runtime
  return typeof raw === 'string' ? raw : 'python'
})
const exitCode = computed(() =>
  typeof result.value.exit_code === 'number' ? result.value.exit_code : null,
)
const stdoutText = computed(() =>
  typeof result.value.stdout === 'string' ? result.value.stdout : '',
)
const stderrText = computed(() =>
  typeof result.value.stderr === 'string' ? result.value.stderr : '',
)

function exitClass(codeValue: number | null): string {
  if (codeValue === null) return 'bg-zinc-100 text-zinc-800'
  return codeValue === 0 ? 'bg-emerald-100 text-emerald-800' : 'bg-rose-100 text-rose-800'
}

async function onCopyCode(): Promise<void> {
  await copyText(code.value)
}

async function onCopyOutput(): Promise<void> {
  await copyText(stdoutText.value || stderrText.value)
}
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="python-panel">
    <header
      class="flex items-center justify-between gap-2 border-b border-border px-3 py-2 text-xs"
    >
      <div class="flex items-center gap-2">
        <span class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">{{ runtime }}</span>
        <span class="rounded px-2 py-0.5" :class="exitClass(exitCode)"
          >exit {{ exitCode ?? 'n/a' }}</span
        >
      </div>
      <div class="flex items-center gap-2">
        <button class="rounded border border-border px-2 py-1 hover:bg-muted" @click="onCopyCode">
          Copy code
        </button>
        <button class="rounded border border-border px-2 py-1 hover:bg-muted" @click="onCopyOutput">
          Copy output
        </button>
      </div>
    </header>

    <div class="space-y-3 p-3">
      <div v-if="code" class="space-y-1">
        <p class="text-xs font-medium text-muted-foreground">Code</p>
        <pre
          class="overflow-x-auto rounded bg-muted/50 p-2 font-mono text-xs"
        ><code>{{ code }}</code></pre>
      </div>

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
        v-if="result.timed_out === true"
        class="rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-800"
      >
        Execution timed out.
      </p>
    </div>
  </section>
</template>
