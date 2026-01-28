<script setup lang="ts">
import { ref, computed } from 'vue'
import { X, PinOff, FileText, Bot, Terminal } from 'lucide-vue-next'
import { useSplitView } from '@/composables/editor/useSplitView'
import { useEditorTabs } from '@/composables/editor/useEditorTabs'
import { Button } from '@/components/ui/button'
import SkillEditor from '@/components/workspace/SkillEditor.vue'
import AgentEditor from '@/components/workspace/AgentEditor.vue'
import TerminalView from './TerminalView.vue'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TerminalSession } from '@/types/generated/TerminalSession'

const emit = defineEmits<{
  save: []
}>()

const { isEnabled, pinnedTabId, splitWidth, unpinTab, setSplitWidth } = useSplitView()

const { tabs, closeTab } = useEditorTabs()

// Get the pinned tab
const pinnedTab = computed(() => tabs.value.find((t) => t.id === pinnedTabId.value))

// Get tab icon
function getTabIcon(type: string) {
  switch (type) {
    case 'skill':
      return FileText
    case 'agent':
      return Bot
    case 'terminal':
      return Terminal
    default:
      return FileText
  }
}

// Resize handling
const isDragging = ref(false)
const startX = ref(0)
const startWidth = ref(splitWidth.value)

function startDragging(event: MouseEvent) {
  isDragging.value = true
  startX.value = event.clientX
  startWidth.value = splitWidth.value

  document.addEventListener('mousemove', handleMouseMove)
  document.addEventListener('mouseup', stopDragging)
  document.body.style.cursor = 'ew-resize'
  document.body.style.userSelect = 'none'
}

function handleMouseMove(event: MouseEvent) {
  if (!isDragging.value) return

  // Moving left increases width (since panel is on right)
  const deltaX = startX.value - event.clientX
  const newWidth = startWidth.value + deltaX
  setSplitWidth(newWidth)
}

function stopDragging() {
  isDragging.value = false
  document.removeEventListener('mousemove', handleMouseMove)
  document.removeEventListener('mouseup', stopDragging)
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}

// Close and unpin
function handleClose() {
  if (pinnedTabId.value) {
    closeTab(pinnedTabId.value)
  }
  unpinTab()
}

// Handle skill save
function handleSkillSave() {
  emit('save')
}
</script>

<template>
  <div
    v-if="isEnabled && pinnedTab"
    data-testid="split-view-panel"
    class="h-full flex shrink-0 border-l"
    :style="{ width: `${splitWidth}px` }"
  >
    <!-- Resize handle - w-0 keeps layout unchanged, absolute positioning expands hit area -->
    <div class="relative w-0">
      <div
        class="absolute inset-y-0 -left-1.5 w-3 cursor-ew-resize flex items-center justify-center group z-10"
        @mousedown="startDragging"
      >
        <!-- Visual line stays 1px -->
        <div class="w-px h-full bg-border group-hover:bg-primary/50 group-active:bg-primary transition-colors" />
      </div>
    </div>

    <!-- Content -->
    <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
      <!-- Header -->
      <div class="h-10 border-b bg-muted/30 flex items-center justify-between px-3 shrink-0">
        <div class="flex items-center gap-2 min-w-0">
          <component
            :is="getTabIcon(pinnedTab.type)"
            :size="14"
            class="shrink-0 text-muted-foreground"
          />
          <span class="text-sm font-medium truncate">{{ pinnedTab.name }}</span>
          <span v-if="pinnedTab.isDirty" class="text-muted-foreground">*</span>
        </div>
        <div class="flex items-center gap-1">
          <Button variant="ghost" size="icon" class="h-6 w-6" title="Unpin" @click="unpinTab">
            <PinOff :size="14" />
          </Button>
          <Button variant="ghost" size="icon" class="h-6 w-6" title="Close" @click="handleClose">
            <X :size="14" />
          </Button>
        </div>
      </div>

      <!-- Editor content -->
      <div class="flex-1 overflow-hidden">
        <SkillEditor
          v-if="pinnedTab.type === 'skill' && pinnedTab.data"
          :skill="pinnedTab.data as Skill"
          :show-header="false"
          @save="handleSkillSave"
        />
        <AgentEditor
          v-else-if="pinnedTab.type === 'agent' && pinnedTab.data"
          :agent="pinnedTab.data as StoredAgent"
          :show-header="false"
          @save="handleSkillSave"
        />
        <TerminalView
          v-else-if="pinnedTab.type === 'terminal' && pinnedTab.data"
          :tab-id="pinnedTab.id"
          :session="pinnedTab.data as TerminalSession"
        />
      </div>
    </div>
  </div>
</template>
