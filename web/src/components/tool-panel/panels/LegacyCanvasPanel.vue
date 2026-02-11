<script setup lang="ts">
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import DOMPurify from 'dompurify'

const props = defineProps<{
  data: Record<string, unknown>
}>()

function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, { ADD_ATTR: ['target', 'rel'] })
}

function formatJson(value: unknown): string {
  if (typeof value === 'string') {
    try {
      return JSON.stringify(JSON.parse(value), null, 2)
    } catch {
      return value
    }
  }
  return JSON.stringify(value, null, 2)
}
</script>

<template>
  <div>
    <MarkdownRenderer
      v-if="String(props.data.content_type ?? 'markdown') === 'markdown'"
      :content="String(props.data.content ?? '')"
    />
    <pre
      v-else-if="String(props.data.content_type) === 'code'"
      class="text-sm font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words"
    ><code>{{ props.data.content ?? '' }}</code></pre>
    <pre
      v-else-if="String(props.data.content_type) === 'json'"
      class="text-sm font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words"
    ><code>{{ formatJson(props.data.content ?? '') }}</code></pre>
    <div
      v-else-if="String(props.data.content_type) === 'html'"
      class="prose prose-sm dark:prose-invert max-w-none"
      v-html="sanitizeHtml(String(props.data.content ?? ''))"
    />
    <pre
      v-else
      class="text-sm font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words"
    ><code>{{ formatJson(props.data) }}</code></pre>
  </div>
</template>
