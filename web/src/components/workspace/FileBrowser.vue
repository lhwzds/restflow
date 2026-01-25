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
  Loader2,
  Plus,
  Tag,
  Bot,
  Trash2,
} from 'lucide-vue-next'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import type { FileItem } from '@/types/workspace'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const props = defineProps<{
  currentPath: string
  selectedId: string | null
  items: FileItem[]
  isLoading?: boolean
  createLabel?: string
  previewType?: 'skill' | 'agent'
}>()

const emit = defineEmits<{
  navigate: [path: string]
  select: [item: FileItem]
  open: [item: FileItem]
  create: []
  delete: [item: FileItem]
}>()

const viewMode = ref<'grid' | 'list'>('grid')
const searchQuery = ref('')
const openPopoverId = ref<string | null>(null)

const filteredItems = computed(() => {
  if (!searchQuery.value) return props.items
  const query = searchQuery.value.toLowerCase()
  return props.items.filter((item) => item.name.toLowerCase().includes(query))
})

const pathSegments = computed(() => {
  return props.currentPath.split('/').filter(Boolean)
})

const canGoBack = computed(() => pathSegments.value.length > 1)
const canGoForward = ref(false)

const goBack = () => {
  if (canGoBack.value) {
    const newPath = pathSegments.value.slice(0, -1).join('/')
    emit('navigate', newPath || pathSegments.value[0] || 'agents')
  }
}

const navigateToSegment = (index: number) => {
  const newPath = pathSegments.value.slice(0, index + 1).join('/')
  emit('navigate', newPath)
}

const onItemClick = (item: FileItem) => {
  emit('select', item)
  openPopoverId.value = item.id
}

const onItemDblClick = (item: FileItem) => {
  openPopoverId.value = null
  if (item.isDirectory) {
    emit('navigate', item.path)
  } else {
    emit('open', item)
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

// Preview helpers
function isSkill(data: unknown): data is Skill {
  return data !== null && typeof data === 'object' && 'content' in data
}

function isAgent(data: unknown): data is StoredAgent {
  return data !== null && typeof data === 'object' && 'agent' in data
}

function getPreviewContent(item: FileItem): string {
  if (!item.data) return ''
  let content = ''
  if (isSkill(item.data)) {
    content = item.data.content || ''
  } else if (isAgent(item.data)) {
    content = item.data.agent.prompt || '*No system prompt*'
  }
  const html = marked.parse(content, { async: false }) as string
  return DOMPurify.sanitize(html)
}

function getTags(item: FileItem): string[] {
  if (!item.data || !isSkill(item.data)) return []
  return item.data.tags || []
}

function getAgentInfo(item: FileItem) {
  if (!item.data || !isAgent(item.data)) return null
  return {
    model: item.data.agent.model,
    temperature: item.data.agent.temperature,
  }
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Toolbar -->
    <div class="h-11 border-b flex items-center px-3 gap-2">
      <!-- Navigation -->
      <div class="flex gap-1">
        <Button size="icon" variant="ghost" class="h-7 w-7" :disabled="!canGoBack" @click="goBack">
          <ChevronLeft :size="16" />
        </Button>
        <Button size="icon" variant="ghost" class="h-7 w-7" :disabled="!canGoForward">
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
          <span
            :class="index === pathSegments.length - 1 ? 'font-medium' : 'text-muted-foreground'"
          >
            {{ segment }}
          </span>
        </button>
      </div>

      <!-- Search -->
      <div class="relative w-40">
        <Search :size="14" class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
        <Input v-model="searchQuery" placeholder="Search..." class="h-7 pl-7 text-sm" />
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

      <!-- Create Button -->
      <Button v-if="createLabel" size="sm" class="h-7" @click="emit('create')">
        <Plus :size="14" class="mr-1" />
        {{ createLabel }}
      </Button>
    </div>

    <!-- File List -->
    <div class="flex-1 overflow-auto p-4 relative">
      <!-- Item Count -->
      <span
        class="absolute top-2 right-4 text-xs text-muted-foreground bg-background/80 px-2 py-0.5 rounded"
      >
        {{ filteredItems.length }} items
      </span>

      <!-- Grid View -->
      <div
        v-if="viewMode === 'grid'"
        class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4"
      >
        <Popover
          v-for="item in filteredItems"
          :key="item.id"
          :open="openPopoverId === item.id"
          @update:open="(open: boolean) => (openPopoverId = open ? item.id : null)"
        >
          <PopoverTrigger as-child>
            <button
              :class="
                cn(
                  'group relative flex flex-col items-center p-3 rounded-lg cursor-pointer transition-all',
                  selectedId === item.id ? 'bg-primary/10 ring-2 ring-primary' : 'hover:bg-muted',
                )
              "
              @click="onItemClick(item)"
              @dblclick="onItemDblClick(item)"
            >
              <div class="w-14 h-14 flex items-center justify-center mb-2">
                <Folder v-if="item.isDirectory" class="w-12 h-12 text-blue-500 fill-blue-500/20" />
                <FileText v-else class="w-10 h-10 text-muted-foreground" />
              </div>
              <span class="text-sm text-center truncate w-full">{{ item.name }}</span>
              <span class="text-xs text-muted-foreground">
                {{ item.isDirectory ? `${item.childCount} items` : formatDate(item.updatedAt) }}
              </span>
              <!-- Delete button (show on hover) -->
              <Button
                v-if="!item.isDirectory"
                variant="ghost"
                size="icon"
                class="absolute top-1 right-1 h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-destructive"
                title="Delete"
                @click.stop="emit('delete', item)"
              >
                <Trash2 :size="14" />
              </Button>
            </button>
          </PopoverTrigger>

          <PopoverContent v-if="!item.isDirectory" class="w-72 p-0" side="right" :side-offset="8">
            <!-- Header -->
            <div class="px-3 py-2 border-b flex items-center gap-2">
              <FileText v-if="previewType === 'skill'" :size="16" class="text-muted-foreground" />
              <Bot v-else :size="16" class="text-muted-foreground" />
              <span class="font-medium text-sm truncate">{{ item.name }}</span>
            </div>

            <!-- Tags -->
            <div
              v-if="getTags(item).length > 0"
              class="px-3 py-1.5 border-b flex items-center gap-1.5 flex-wrap"
            >
              <Tag :size="12" class="text-muted-foreground shrink-0" />
              <Badge
                v-for="tag in getTags(item)"
                :key="tag"
                variant="secondary"
                class="text-[10px] px-1.5 py-0"
              >
                {{ tag }}
              </Badge>
            </div>

            <!-- Agent Info -->
            <div
              v-if="getAgentInfo(item)"
              class="px-3 py-1.5 border-b text-[10px] text-muted-foreground"
            >
              <div><strong>Model:</strong> {{ getAgentInfo(item)?.model }}</div>
              <div v-if="getAgentInfo(item)?.temperature !== undefined">
                <strong>Temperature:</strong> {{ getAgentInfo(item)?.temperature }}
              </div>
            </div>

            <!-- Content -->
            <div class="px-3 py-2 max-h-[150px] overflow-auto">
              <div
                v-html="getPreviewContent(item)"
                class="prose prose-xs dark:prose-invert max-w-none text-xs"
              />
            </div>
          </PopoverContent>
        </Popover>

        <!-- Create new item card -->
        <Card
          v-if="createLabel"
          class="cursor-pointer border-dashed hover:border-primary transition-colors"
          @click="emit('create')"
        >
          <CardContent
            class="flex flex-col items-center justify-center p-6 text-muted-foreground hover:text-foreground transition-colors"
          >
            <Plus :size="32" class="mb-2" />
            <span class="text-sm">{{ createLabel }}</span>
          </CardContent>
        </Card>
      </div>

      <!-- List View -->
      <div v-else class="space-y-1">
        <Popover
          v-for="item in filteredItems"
          :key="item.id"
          :open="openPopoverId === item.id"
          @update:open="(open: boolean) => (openPopoverId = open ? item.id : null)"
        >
          <PopoverTrigger as-child>
            <button
              :class="
                cn(
                  'group w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left',
                  selectedId === item.id ? 'bg-primary/10 ring-1 ring-primary' : 'hover:bg-muted',
                )
              "
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
              <!-- Delete button (show on hover) -->
              <Button
                v-if="!item.isDirectory"
                variant="ghost"
                size="icon"
                class="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-destructive shrink-0"
                title="Delete"
                @click.stop="emit('delete', item)"
              >
                <Trash2 :size="14" />
              </Button>
            </button>
          </PopoverTrigger>

          <PopoverContent v-if="!item.isDirectory" class="w-72 p-0" side="right" :side-offset="8">
            <!-- Same content as grid view -->
            <div class="px-3 py-2 border-b flex items-center gap-2">
              <FileText v-if="previewType === 'skill'" :size="16" class="text-muted-foreground" />
              <Bot v-else :size="16" class="text-muted-foreground" />
              <span class="font-medium text-sm truncate">{{ item.name }}</span>
            </div>
            <div
              v-if="getTags(item).length > 0"
              class="px-3 py-1.5 border-b flex items-center gap-1.5 flex-wrap"
            >
              <Tag :size="12" class="text-muted-foreground shrink-0" />
              <Badge
                v-for="tag in getTags(item)"
                :key="tag"
                variant="secondary"
                class="text-[10px] px-1.5 py-0"
              >
                {{ tag }}
              </Badge>
            </div>
            <div
              v-if="getAgentInfo(item)"
              class="px-3 py-1.5 border-b text-[10px] text-muted-foreground"
            >
              <div><strong>Model:</strong> {{ getAgentInfo(item)?.model }}</div>
            </div>
            <div class="px-3 py-2 max-h-[150px] overflow-auto">
              <div
                v-html="getPreviewContent(item)"
                class="prose prose-xs dark:prose-invert max-w-none text-xs"
              />
            </div>
          </PopoverContent>
        </Popover>

        <!-- Create new item row -->
        <button
          v-if="createLabel"
          class="w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left border-2 border-dashed hover:border-primary hover:bg-muted/50"
          @click="emit('create')"
        >
          <Plus :size="20" class="text-muted-foreground shrink-0" />
          <span class="flex-1 text-sm text-muted-foreground">{{ createLabel }}</span>
        </button>
      </div>

      <!-- Loading State -->
      <div
        v-if="isLoading"
        class="flex flex-col items-center justify-center h-full text-muted-foreground"
      >
        <Loader2 :size="32" class="mb-2 animate-spin" />
        <span class="text-sm">Loading...</span>
      </div>

      <!-- Empty State -->
      <div
        v-else-if="filteredItems.length === 0"
        class="flex flex-col items-center justify-center h-full text-muted-foreground"
      >
        <Folder :size="48" class="mb-2 opacity-50" />
        <span class="text-sm">No files found</span>
      </div>
    </div>
  </div>
</template>
