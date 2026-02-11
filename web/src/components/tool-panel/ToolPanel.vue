<script setup lang="ts">
import { computed, ref } from 'vue'
import { X, GripVertical, ChevronLeft, ChevronRight, PanelRight } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import TerminalPanel from './panels/TerminalPanel.vue'
import HttpPanel from './panels/HttpPanel.vue'
import FilePanel from './panels/FilePanel.vue'
import SearchPanel from './panels/SearchPanel.vue'
import PythonPanel from './panels/PythonPanel.vue'
import WebPanel from './panels/WebPanel.vue'
import NotificationPanel from './panels/NotificationPanel.vue'
import GenericJsonPanel from './panels/GenericJsonPanel.vue'
import LegacyCanvasPanel from './panels/LegacyCanvasPanel.vue'
import type { ToolPanelType } from '@/composables/workspace/useToolPanel'

const props = defineProps<{
  panelType: ToolPanelType
  title: string
  toolName: string
  data: Record<string, unknown>
  canNavigatePrev: boolean
  canNavigateNext: boolean
}>()

const emit = defineEmits<{
  close: []
  navigate: [direction: 'prev' | 'next']
}>()

const panelWidth = ref(420)
const isDragging = ref(false)
const MIN_WIDTH = 320
const MAX_WIDTH = 860

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

const panelComponent = computed(() => {
  const map: Record<ToolPanelType, unknown> = {
    terminal: TerminalPanel,
    http: HttpPanel,
    file: FilePanel,
    search: SearchPanel,
    python: PythonPanel,
    web: WebPanel,
    notification: NotificationPanel,
    generic: GenericJsonPanel,
    canvas: LegacyCanvasPanel,
  }

  return map[props.panelType] ?? GenericJsonPanel
})
</script>

<template>
  <div
    class="shrink-0 border-l border-border bg-background flex flex-col relative"
    :style="{ width: `${panelWidth}px` }"
  >
    <div
      class="absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/20 transition-colors z-10"
      :class="{ 'bg-primary/30': isDragging }"
      @mousedown="startDrag"
    >
      <div class="absolute left-0 top-1/2 -translate-y-1/2 opacity-0 hover:opacity-50">
        <GripVertical :size="12" />
      </div>
    </div>

    <div class="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
      <PanelRight :size="14" class="text-muted-foreground shrink-0" />
      <span class="text-sm font-medium truncate flex-1">{{ props.title || props.toolName || 'Tool Panel' }}</span>
      <Button
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :disabled="!props.canNavigatePrev"
        @click="emit('navigate', 'prev')"
      >
        <ChevronLeft :size="14" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        class="h-6 w-6"
        :disabled="!props.canNavigateNext"
        @click="emit('navigate', 'next')"
      >
        <ChevronRight :size="14" />
      </Button>
      <button
        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        @click="emit('close')"
      >
        <X :size="14" />
      </button>
    </div>

    <div class="flex-1 overflow-auto p-4">
      <component :is="panelComponent" :data="props.data" />
    </div>
  </div>
</template>
