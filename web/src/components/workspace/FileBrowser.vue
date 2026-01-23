<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  ChevronLeft,
  ChevronRight,
  LayoutGrid,
  List,
  Folder,
  FileText,
  Search,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { FileItem } from '@/types/workspace'
import { getMockFiles } from '@/mocks/workspace'

const props = defineProps<{
  currentPath: string
  selected: string | null
}>()

const emit = defineEmits<{
  navigate: [path: string]
  select: [path: string]
}>()

const viewMode = ref<'grid' | 'list'>('grid')
const searchQuery = ref('')

// Get files from mock data (will be replaced with actual file system API)
const items = computed<FileItem[]>(() => getMockFiles(props.currentPath))

const filteredItems = computed(() => {
  if (!searchQuery.value) return items.value
  const query = searchQuery.value.toLowerCase()
  return items.value.filter(item => item.name.toLowerCase().includes(query))
})

const pathSegments = computed(() => {
  return props.currentPath.split('/').filter(Boolean)
})

const canGoBack = computed(() => pathSegments.value.length > 1)
const canGoForward = ref(false)

const goBack = () => {
  if (canGoBack.value) {
    const newPath = pathSegments.value.slice(0, -1).join('/')
    // Preserve root path (agents or skills)
    emit('navigate', newPath || pathSegments.value[0] || 'agents')
  }
}

const navigateToSegment = (index: number) => {
  const newPath = pathSegments.value.slice(0, index + 1).join('/')
  emit('navigate', newPath)
}

const onItemClick = (item: FileItem) => {
  emit('select', item.path)
}

const onItemDblClick = (item: FileItem) => {
  if (item.isDirectory) {
    emit('navigate', item.path)
  }
}

const formatDate = (timestamp?: number) => {
  if (!timestamp) return ''
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 86400000) return 'Today'
  if (diff < 172800000) return 'Yesterday'
  return `${Math.floor(diff / 86400000)} days ago`
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Toolbar -->
    <div class="h-11 border-b flex items-center px-3 gap-2">
      <!-- Navigation -->
      <div class="flex gap-1">
        <Button
          size="icon"
          variant="ghost"
          class="h-7 w-7"
          :disabled="!canGoBack"
          @click="goBack"
        >
          <ChevronLeft :size="16" />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          class="h-7 w-7"
          :disabled="!canGoForward"
        >
          <ChevronRight :size="16" />
        </Button>
      </div>

      <!-- Breadcrumb -->
      <div class="flex-1 flex items-center gap-1 text-sm">
        <button
          v-for="(segment, index) in pathSegments"
          :key="index"
          class="flex items-center hover:text-primary transition-colors"
          @click="navigateToSegment(index)"
        >
          <span v-if="index > 0" class="mx-1 text-muted-foreground">/</span>
          <span :class="index === pathSegments.length - 1 ? 'font-medium' : 'text-muted-foreground'">
            {{ segment }}
          </span>
        </button>
      </div>

      <!-- Search -->
      <div class="relative w-40">
        <Search :size="14" class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
        <Input
          v-model="searchQuery"
          placeholder="Search..."
          class="h-7 pl-7 text-sm"
        />
      </div>

      <!-- View Toggle -->
      <div class="flex gap-0.5 border rounded-md p-0.5">
        <Button
          size="icon"
          :variant="viewMode === 'list' ? 'secondary' : 'ghost'"
          class="h-6 w-6"
          @click="viewMode = 'list'"
        >
          <List :size="14" />
        </Button>
        <Button
          size="icon"
          :variant="viewMode === 'grid' ? 'secondary' : 'ghost'"
          class="h-6 w-6"
          @click="viewMode = 'grid'"
        >
          <LayoutGrid :size="14" />
        </Button>
      </div>
    </div>

    <!-- File List -->
    <div class="flex-1 overflow-auto p-4 relative">
      <!-- Item Count -->
      <span class="absolute top-2 right-4 text-xs text-muted-foreground bg-background/80 px-2 py-0.5 rounded">
        {{ filteredItems.length }} items
      </span>
      <!-- Grid View -->
      <div
        v-if="viewMode === 'grid'"
        class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4"
      >
        <button
          v-for="item in filteredItems"
          :key="item.path"
          :class="cn(
            'flex flex-col items-center p-3 rounded-lg cursor-pointer transition-all',
            selected === item.path
              ? 'bg-primary/10 ring-2 ring-primary'
              : 'hover:bg-muted'
          )"
          @click="onItemClick(item)"
          @dblclick="onItemDblClick(item)"
        >
          <!-- Icon -->
          <div class="w-14 h-14 flex items-center justify-center mb-2">
            <Folder
              v-if="item.isDirectory"
              class="w-12 h-12 text-blue-500 fill-blue-500/20"
            />
            <FileText v-else class="w-10 h-10 text-muted-foreground" />
          </div>

          <!-- Name -->
          <span class="text-sm text-center truncate w-full">{{ item.name }}</span>

          <!-- Description -->
          <span class="text-xs text-muted-foreground">
            {{ item.isDirectory ? `${item.childCount} items` : formatDate(item.updatedAt) }}
          </span>
        </button>
      </div>

      <!-- List View -->
      <div v-else class="space-y-1">
        <button
          v-for="item in filteredItems"
          :key="item.path"
          :class="cn(
            'w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left',
            selected === item.path
              ? 'bg-primary/10 ring-1 ring-primary'
              : 'hover:bg-muted'
          )"
          @click="onItemClick(item)"
          @dblclick="onItemDblClick(item)"
        >
          <Folder
            v-if="item.isDirectory"
            :size="20"
            class="text-blue-500 fill-blue-500/20 shrink-0"
          />
          <FileText v-else :size="20" class="text-muted-foreground shrink-0" />

          <span class="flex-1 text-sm truncate">{{ item.name }}</span>

          <span class="text-xs text-muted-foreground">
            {{ item.isDirectory ? `${item.childCount} items` : formatDate(item.updatedAt) }}
          </span>
        </button>
      </div>

      <!-- Empty State -->
      <div
        v-if="filteredItems.length === 0"
        class="flex flex-col items-center justify-center h-full text-muted-foreground"
      >
        <Folder :size="48" class="mb-2 opacity-50" />
        <span class="text-sm">No files found</span>
      </div>
    </div>

  </div>
</template>
