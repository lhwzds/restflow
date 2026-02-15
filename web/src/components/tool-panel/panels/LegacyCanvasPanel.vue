<script setup lang="ts">
import { ref } from 'vue'
import { X, GripVertical, FileText, Code, Braces, Globe } from 'lucide-vue-next'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import DOMPurify from 'dompurify'

const props = defineProps<{
  title: string
  content: string
  contentType: 'markdown' | 'code' | 'json' | 'html'
}>()

const emit = defineEmits<{
  close: []
}>()

const panelWidth = ref(400)
const isDragging = ref(false)
const MIN_WIDTH = 280
const MAX_WIDTH = 800

function startDrag(e: MouseEvent) {
  isDragging.value = true
  const startX = e.clientX
  const startWidth = panelWidth.value

  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    panelWidth.value = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
  }

  function onUp() {
    isDragging.value = false
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }

  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

const contentTypeIcons = {
  markdown: FileText,
  code: Code,
  json: Braces,
  html: Globe,
}

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
  <div
    class="relative flex shrink-0 flex-col border-l border-border bg-background"
    :style="{ width: `${panelWidth}px` }"
  >
    <div
      class="absolute bottom-0 left-0 top-0 z-10 w-1 cursor-col-resize transition-colors hover:bg-primary/20"
      :class="{ 'bg-primary/30': isDragging }"
      @mousedown="startDrag"
    >
      <div class="absolute left-0 top-1/2 -translate-y-1/2 opacity-0 hover:opacity-50">
        <GripVertical :size="12" />
      </div>
    </div>

    <div class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
      <component
        :is="contentTypeIcons[props.contentType]"
        :size="14"
        class="shrink-0 text-muted-foreground"
      />
      <span class="flex-1 truncate text-sm font-medium">{{ props.title || 'Canvas' }}</span>
      <button
        class="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        @click="emit('close')"
      >
        <X :size="14" />
      </button>
    </div>

    <div class="flex-1 overflow-auto p-4">
      <MarkdownRenderer v-if="props.contentType === 'markdown'" :content="props.content" />

      <pre
        v-else-if="props.contentType === 'code'"
        class="overflow-x-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-3 font-mono text-sm"
      ><code>{{ props.content }}</code></pre>

      <pre
        v-else-if="props.contentType === 'json'"
        class="overflow-x-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-3 font-mono text-sm"
      ><code>{{ formatJson(props.content) }}</code></pre>

      <div
        v-else-if="props.contentType === 'html'"
        class="prose prose-sm max-w-none dark:prose-invert"
        v-html="sanitizeHtml(props.content)"
      />
    </div>
  </div>
</template>
