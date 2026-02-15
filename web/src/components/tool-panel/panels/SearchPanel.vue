<script setup lang="ts">
import { computed } from 'vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import { parseToolResult } from '@/components/tool-panel/utils'

interface SearchResult {
  title?: string
  url?: string
  snippet?: string
}

interface SearchPayload {
  provider?: string
  results?: SearchResult[]
}

interface MemoryHit {
  score: string
  content: string
}

const props = defineProps<{
  step: StreamStep
}>()

const parsedResult = computed(() => parseToolResult(props.step))

const webPayload = computed<SearchPayload>(() => {
  const raw = parsedResult.value
  if (typeof raw === 'object' && raw !== null && !Array.isArray(raw)) return raw as SearchPayload
  return {}
})

const memoryHits = computed<MemoryHit[]>(() => {
  if (typeof parsedResult.value !== 'string') return []
  return parsedResult.value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const scoreMatch = line.match(/\[Score:\s*([0-9.]+)\]/i)
      const content = line
        .replace(/^\d+\.\s*/, '')
        .replace(/\[Score:[^\]]+\]\s*/i, '')
        .trim()
      return {
        score: scoreMatch?.[1] ?? '-',
        content,
      }
    })
})

const provider = computed(() => {
  const raw = webPayload.value.provider
  return typeof raw === 'string' ? raw : props.step.name
})

const results = computed(() =>
  Array.isArray(webPayload.value.results) ? webPayload.value.results : [],
)
</script>

<template>
  <section class="rounded-md border border-border bg-background" data-testid="search-panel">
    <header class="flex items-center justify-between border-b border-border px-3 py-2 text-xs">
      <div class="flex items-center gap-2">
        <span class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">{{ provider }}</span>
        <span class="rounded bg-zinc-100 px-2 py-0.5 text-zinc-800">
          {{ results.length || memoryHits.length }} hits
        </span>
      </div>
    </header>

    <div class="space-y-2 p-3">
      <article
        v-for="(item, index) in results"
        :key="index"
        class="rounded border border-border p-2 text-xs"
      >
        <p class="font-medium">{{ item.title || 'Untitled' }}</p>
        <a
          v-if="item.url"
          :href="item.url"
          target="_blank"
          rel="noreferrer"
          class="mt-1 block truncate font-mono text-sky-700 underline"
        >
          {{ item.url }}
        </a>
        <p v-if="item.snippet" class="mt-1 text-muted-foreground">{{ item.snippet }}</p>
      </article>

      <article
        v-for="(hit, index) in memoryHits"
        :key="`m-${index}`"
        class="rounded border border-border p-2 text-xs"
      >
        <p class="text-muted-foreground">Score {{ hit.score }}</p>
        <p class="mt-1">{{ hit.content }}</p>
      </article>

      <p v-if="!results.length && !memoryHits.length" class="text-xs text-muted-foreground">
        No results.
      </p>
    </div>
  </section>
</template>
