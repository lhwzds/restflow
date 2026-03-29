<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { Search, MessageSquare, Bot, Plus, Settings, Loader2 } from 'lucide-vue-next'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { useCommandPalette } from '@/composables/useCommandPalette'
import { listExecutionContainers } from '@/api/execution-console'
import { listAgents } from '@/api/agents'
import type { ExecutionContainerSummary } from '@/types/generated/ExecutionContainerSummary'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const emit = defineEmits<{
  navigateContainer: [id: string]
  navigateAgent: [id: string]
  newSession: []
  openSettings: []
}>()

const palette = useCommandPalette()
const searchQuery = ref('')
const activeIndex = ref(0)
const loading = ref(false)

const containers = ref<ExecutionContainerSummary[]>([])
const agents = ref<StoredAgent[]>([])
const searchInputRef = ref<HTMLInputElement | null>(null)

// Load data when palette opens
watch(
  () => palette.isOpen.value,
  async (open) => {
    if (!open) return
    searchQuery.value = ''
    activeIndex.value = 0
    loading.value = true
    try {
      const [c, a] = await Promise.all([listExecutionContainers(), listAgents()])
      containers.value = c.filter((x) => x.kind === 'workspace')
      agents.value = a
    } finally {
      loading.value = false
    }
    await nextTick()
    searchInputRef.value?.focus()
  },
)

interface PaletteItem {
  id: string
  label: string
  subtitle?: string
  badge?: string
  group: 'sessions' | 'agents' | 'actions'
  icon: typeof MessageSquare
  action: () => void
}

const actionItems: PaletteItem[] = [
  {
    id: 'new-session',
    label: 'New Session',
    badge: 'Action',
    group: 'actions',
    icon: Plus,
    action: () => {
      palette.close()
      emit('newSession')
    },
  },
  {
    id: 'open-settings',
    label: 'Settings',
    badge: 'Action',
    group: 'actions',
    icon: Settings,
    action: () => {
      palette.close()
      emit('openSettings')
    },
  },
]

const allItems = computed<PaletteItem[]>(() => {
  const q = searchQuery.value.toLowerCase()

  const sessionItems: PaletteItem[] = containers.value
    .filter((c) => !q || c.title.toLowerCase().includes(q))
    .map((c) => ({
      id: `session-${c.id}`,
      label: c.title,
      subtitle: c.subtitle ?? undefined,
      badge: 'Session',
      group: 'sessions' as const,
      icon: MessageSquare,
      action: () => {
        palette.close()
        emit('navigateContainer', c.id)
      },
    }))

  const agentItems: PaletteItem[] = agents.value
    .filter((a) => !q || a.name.toLowerCase().includes(q))
    .map((a) => ({
      id: `agent-${a.id}`,
      label: a.name,
      badge: 'Agent',
      group: 'agents' as const,
      icon: Bot,
      action: () => {
        palette.close()
        emit('navigateAgent', a.id)
      },
    }))

  const filteredActions = actionItems.filter(
    (item) => !q || item.label.toLowerCase().includes(q),
  )

  return [...sessionItems, ...agentItems, ...filteredActions]
})

// Reset active index when items change
watch(allItems, () => {
  activeIndex.value = 0
})

function onKeydown(e: KeyboardEvent) {
  const items = allItems.value
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    activeIndex.value = Math.min(activeIndex.value + 1, items.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    activeIndex.value = Math.max(activeIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    items[activeIndex.value]?.action()
  }
}

const groupLabels: Record<string, string> = {
  sessions: 'Sessions',
  agents: 'Agents',
  actions: 'Actions',
}

// Group items for display
const groupedItems = computed(() => {
  const groups = new Map<string, PaletteItem[]>()
  for (const item of allItems.value) {
    const group = groups.get(item.group) ?? []
    group.push(item)
    groups.set(item.group, group)
  }
  return groups
})

// Flat index offset for each group's items (for activeIndex highlight)
const flatIndexMap = computed(() => {
  const map = new Map<string, number>()
  let i = 0
  for (const item of allItems.value) {
    map.set(item.id, i++)
  }
  return map
})
</script>

<template>
  <Dialog :open="palette.isOpen.value" @update:open="(v) => !v && palette.close()">
    <DialogContent
      class="p-0 gap-0 overflow-hidden max-w-[42rem]"
      data-testid="command-palette"
      :hide-close-button="true"
    >
      <!-- Search input -->
      <div class="flex items-center gap-2 border-b border-border px-4 py-3">
        <Search :size="16" class="shrink-0 text-muted-foreground" />
        <input
          ref="searchInputRef"
          v-model="searchQuery"
          placeholder="Search sessions, agents, actions..."
          class="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          data-testid="command-palette-input"
          @keydown="onKeydown"
        />
        <Loader2 v-if="loading" :size="14" class="animate-spin text-muted-foreground shrink-0" />
      </div>

      <!-- Results -->
      <div class="max-h-[360px] overflow-y-auto py-2" data-testid="command-palette-results">
        <template v-if="allItems.length === 0 && !loading">
          <p class="px-4 py-6 text-center text-sm text-muted-foreground">No results found.</p>
        </template>

        <template v-for="[group, items] in groupedItems" :key="group">
          <div class="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
            {{ groupLabels[group] ?? group }}
          </div>
          <button
            v-for="item in items"
            :key="item.id"
            :data-testid="`command-palette-item-${item.id}`"
            class="flex w-full items-center gap-3 px-3 py-2 text-sm transition-colors"
            :class="
              flatIndexMap.get(item.id) === activeIndex
                ? 'bg-primary/10 text-foreground'
                : 'text-foreground/80 hover:bg-muted/60'
            "
            @click="item.action()"
            @mouseenter="activeIndex = flatIndexMap.get(item.id) ?? 0"
          >
            <component :is="item.icon" :size="14" class="shrink-0 text-muted-foreground" />
            <span class="flex-1 truncate text-left">{{ item.label }}</span>
            <span
              v-if="item.subtitle"
              class="truncate text-xs text-muted-foreground max-w-[8rem]"
            >
              {{ item.subtitle }}
            </span>
            <span
              v-if="item.badge"
              class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground"
            >
              {{ item.badge }}
            </span>
          </button>
        </template>
      </div>

      <!-- Footer hint -->
      <div class="flex items-center gap-3 border-t border-border px-4 py-2 text-[11px] text-muted-foreground/60">
        <span><kbd class="font-mono">↑↓</kbd> navigate</span>
        <span><kbd class="font-mono">↵</kbd> select</span>
        <span><kbd class="font-mono">Esc</kbd> close</span>
      </div>
    </DialogContent>
  </Dialog>
</template>
