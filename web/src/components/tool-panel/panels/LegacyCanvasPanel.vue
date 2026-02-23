<script setup lang="ts">
import { computed } from 'vue'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import DOMPurify from 'dompurify'

const props = defineProps<{
  data: Record<string, unknown>
}>()

const content = computed(() =>
  typeof props.data.content === 'string' ? props.data.content : '',
)
const contentType = computed<'markdown' | 'code' | 'json' | 'html'>(() => {
  const ct = props.data.content_type ?? props.data.contentType
  if (ct === 'markdown' || ct === 'code' || ct === 'json' || ct === 'html') return ct
  return 'markdown'
})

function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, { ADD_ATTR: ['target', 'rel'] })
}

function formatJson(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2)
  } catch {
    return json
  }
}
</script>

<template>
  <div>
    <MarkdownRenderer v-if="contentType === 'markdown'" :content="content" />

    <pre
      v-else-if="contentType === 'code'"
      class="overflow-x-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-3 font-mono text-sm"
    ><code>{{ content }}</code></pre>

    <pre
      v-else-if="contentType === 'json'"
      class="overflow-x-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-3 font-mono text-sm"
    ><code>{{ formatJson(content) }}</code></pre>

    <div
      v-else-if="contentType === 'html'"
      class="prose prose-sm max-w-none dark:prose-invert"
      v-html="sanitizeHtml(content)"
    />
  </div>
</template>
