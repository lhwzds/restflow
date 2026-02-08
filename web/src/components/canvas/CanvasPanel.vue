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

// Draggable width
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
    class="shrink-0 border-l border-border bg-background flex flex-col relative"
    :style="{ width: `${panelWidth}px` }"
  >
    <!-- Drag handle -->
    <div
      class="absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/20 transition-colors z-10"
      :class="{ 'bg-primary/30': isDragging }"
      @mousedown="startDrag"
    >
      <div class="absolute left-0 top-1/2 -translate-y-1/2 opacity-0 hover:opacity-50">
        <GripVertical :size="12" />
      </div>
    </div>

    <!-- Header -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
      <component :is="contentTypeIcons[props.contentType]" :size="14" class="text-muted-foreground shrink-0" />
      <span class="text-sm font-medium truncate flex-1">{{ props.title || 'Canvas' }}</span>
      <button
        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        @click="emit('close')"
      >
        <X :size="14" />
      </button>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Markdown -->
      <MarkdownRenderer
        v-if="props.contentType === 'markdown'"
        :content="props.content"
      />

      <!-- Code -->
      <pre
        v-else-if="props.contentType === 'code'"
        class="text-sm font-mono bg-muted/50 rounded-md p-3 overflow-x-auto whitespace-pre-wrap break-words"
      ><code>{{ props.content }}</code></pre>

      <!-- JSON -->
      <pre
        v-else-if="props.contentType === 'json'"
        class="text-sm font-mono bg-muted/50 rounded-md p-3 overflow-x-auto whitespace-pre-wrap break-words"
      ><code>{{ formatJson(props.content) }}</code></pre>

      <!-- HTML (sanitized) -->
      <div
        v-else-if="props.contentType === 'html'"
        class="prose prose-sm dark:prose-invert max-w-none"
        v-html="sanitizeHtml(props.content)"
      />
    </div>
  </div>
</template>
