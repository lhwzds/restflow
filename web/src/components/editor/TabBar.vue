<script setup lang="ts">
import { X, Plus, FileText, Bot, Terminal, Pin } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import type { EditorTab } from '@/composables/editor/useEditorTabs'
import { useSplitView } from '@/composables/editor/useSplitView'

defineProps<{
  tabs: EditorTab[]
  activeTabId: string | null
}>()

const emit = defineEmits<{
  select: [tabId: string]
  close: [tabId: string]
  newSkill: []
  newAgent: []
  newTerminal: []
  pin: [tabId: string]
}>()

// Split view uses Pin button instead of drag-and-drop due to Tauri WebView
// limitations. See SkillWorkspace.vue for detailed explanation.
const { isPinned } = useSplitView()

function getTabIcon(type: EditorTab['type']) {
  switch (type) {
    case 'skill':
      return FileText
    case 'agent':
      return Bot
    case 'terminal':
      return Terminal
  }
}
</script>

<template>
  <div class="flex items-center gap-1 px-2 overflow-x-auto" data-testid="tab-bar">
    <!-- Tabs -->
    <div
      v-for="tab in tabs"
      :key="tab.id"
      :data-tab-id="tab.id"
      :class="
        cn(
          'group flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-t-md cursor-pointer border border-b-0 transition-colors',
          activeTabId === tab.id
            ? 'bg-background border-border'
            : 'bg-muted/50 border-transparent hover:bg-muted',
          isPinned(tab.id) && 'opacity-50',
        )
      "
      @click="emit('select', tab.id)"
    >
      <component :is="getTabIcon(tab.type)" :size="14" class="shrink-0 text-muted-foreground" />
      <span class="truncate max-w-[120px]">{{ tab.name }}</span>
      <!-- Dirty indicator -->
      <span v-if="tab.isDirty && !isPinned(tab.id)" class="text-muted-foreground">*</span>
      <!-- Pin button (show on hover, or always show if pinned) -->
      <button
        :class="[
          'p-0.5 rounded transition-opacity',
          isPinned(tab.id)
            ? 'text-primary'
            : 'hover:bg-muted-foreground/20 opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-primary',
        ]"
        :title="isPinned(tab.id) ? 'Unpin from split view' : 'Pin to split view'"
        @click.stop="emit('pin', tab.id)"
      >
        <Pin :size="12" />
      </button>
      <!-- Close button -->
      <button
        class="p-0.5 rounded hover:bg-muted-foreground/20 opacity-0 group-hover:opacity-100 transition-opacity"
        title="Close"
        @click.stop="emit('close', tab.id)"
      >
        <X :size="12" />
      </button>
    </div>

    <!-- New Item Dropdown -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button variant="ghost" size="icon" class="h-7 w-7 shrink-0" title="New...">
          <Plus :size="14" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem @click="emit('newSkill')">
          <FileText :size="14" class="mr-2" />
          New Skill
        </DropdownMenuItem>
        <DropdownMenuItem @click="emit('newAgent')">
          <Bot :size="14" class="mr-2" />
          New Agent
        </DropdownMenuItem>
        <DropdownMenuItem @click="emit('newTerminal')">
          <Terminal :size="14" class="mr-2" />
          New Terminal
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  </div>
</template>
