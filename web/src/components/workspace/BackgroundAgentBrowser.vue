<!--
  BackgroundAgentBrowser Component - Design Decisions:

  1. Follows the same pattern as TerminalBrowser for consistency
  2. searchQuery and viewMode are PROPS, managed in parent (SkillWorkspace)
  3. Uses existing BackgroundAgentCard component for display
  4. Uses CreateBackgroundAgentDialog for creating new background agents
  5. Integrates with backgroundAgentStore for state management
-->
<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { CalendarClock, Plus, Trash2, Loader2, Play, Pause } from 'lucide-vue-next'
import { storeToRefs } from 'pinia'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'
import { CreateBackgroundAgentDialog } from '@/components/background-agent'
import type { BackgroundAgent, BackgroundAgentStatus } from '@/types/background-agent'
import type { CreateBackgroundAgentRequest } from '@/api/background-agent'
import { formatSchedule, formatBackgroundAgentStatus } from '@/api/background-agent'

const props = defineProps<{
  searchQuery: string
  viewMode: 'grid' | 'list'
}>()

const store = useBackgroundAgentStore()
const { agents, isLoading } = storeToRefs(store)
const toast = useToast()
const { confirm } = useConfirm()

// Local state
const showCreateDialog = ref(false)
const loadingBackgroundAgentIds = ref<Set<string>>(new Set())

// Load background agents on mount
onMounted(async () => {
  await store.fetchBackgroundAgents()
  await store.startRealtimeSync()
})

onUnmounted(() => {
  store.stopRealtimeSync()
})

// Filter background agents by search query
const filteredBackgroundAgents = computed(() => {
  if (!props.searchQuery) return agents.value
  const query = props.searchQuery.toLowerCase()
  return agents.value.filter(
    (backgroundAgent) =>
      backgroundAgent.name.toLowerCase().includes(query) ||
      backgroundAgent.description?.toLowerCase().includes(query),
  )
})

// Get status badge variant
function getStatusVariant(
  status: BackgroundAgentStatus,
): 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'info' {
  const variantMap: Record<BackgroundAgentStatus, 'success' | 'info' | 'default' | 'destructive'> =
    {
      active: 'success',
      paused: 'info',
      running: 'default',
      completed: 'success',
      failed: 'destructive',
    }
  return variantMap[status] || 'default'
}

// Get status indicator class
function getStatusIndicatorClass(status: BackgroundAgentStatus): string {
  const classMap: Record<BackgroundAgentStatus, string> = {
    active: 'bg-green-500',
    running: 'bg-blue-500 animate-pulse',
    paused: 'bg-yellow-500',
    completed: 'bg-green-500',
    failed: 'bg-red-500',
  }
  return classMap[status] || 'bg-gray-400'
}

// Format relative time
function formatRelativeTime(timestamp: number | null): string {
  if (!timestamp) return 'Never'

  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const future = diff < 0

  const absDiff = Math.abs(diff)
  const minutes = Math.floor(absDiff / 60000)
  const hours = Math.floor(absDiff / 3600000)
  const days = Math.floor(absDiff / 86400000)

  if (minutes < 1) return future ? 'Soon' : 'Just now'
  if (minutes < 60) return future ? `In ${minutes}m` : `${minutes}m ago`
  if (hours < 24) return future ? `In ${hours}h` : `${hours}h ago`
  if (days < 7) return future ? `In ${days}d` : `${days}d ago`

  return date.toLocaleDateString()
}

// Handle background agent actions
async function handlePauseBackgroundAgent(event: Event, backgroundAgent: BackgroundAgent) {
  event.stopPropagation()
  if (loadingBackgroundAgentIds.value.has(backgroundAgent.id)) return

  loadingBackgroundAgentIds.value.add(backgroundAgent.id)
  try {
    await store.pauseBackgroundAgent(backgroundAgent.id)
    toast.success(`Background agent "${backgroundAgent.name}" paused`)
  } catch (error) {
    toast.error('Failed to pause background agent')
  } finally {
    loadingBackgroundAgentIds.value.delete(backgroundAgent.id)
  }
}

async function handleResumeBackgroundAgent(event: Event, backgroundAgent: BackgroundAgent) {
  event.stopPropagation()
  if (loadingBackgroundAgentIds.value.has(backgroundAgent.id)) return

  loadingBackgroundAgentIds.value.add(backgroundAgent.id)
  try {
    await store.resumeBackgroundAgent(backgroundAgent.id)
    toast.success(`Background agent "${backgroundAgent.name}" resumed`)
  } catch (error) {
    toast.error('Failed to resume background agent')
  } finally {
    loadingBackgroundAgentIds.value.delete(backgroundAgent.id)
  }
}

async function handleDeleteBackgroundAgent(event: Event, backgroundAgent: BackgroundAgent) {
  event.stopPropagation()
  if (loadingBackgroundAgentIds.value.has(backgroundAgent.id)) return

  const confirmed = await confirm({
    title: 'Delete Background Agent',
    description: `Are you sure you want to delete "${backgroundAgent.name}"? This action cannot be undone.`,
    confirmText: 'Delete',
    cancelText: 'Cancel',
    variant: 'destructive',
  })

  if (!confirmed) return

  loadingBackgroundAgentIds.value.add(backgroundAgent.id)
  try {
    await store.deleteBackgroundAgent(backgroundAgent.id)
    toast.success(`Background agent "${backgroundAgent.name}" deleted`)
  } catch (error) {
    toast.error('Failed to delete background agent')
  } finally {
    loadingBackgroundAgentIds.value.delete(backgroundAgent.id)
  }
}

// Handle background agent click (could open details in future)
function handleBackgroundAgentClick(backgroundAgent: BackgroundAgent) {
  // TODO: Open background agent details in editor panel or dialog
  console.log('Background agent clicked:', backgroundAgent.id)
}

// Handle background agent creation
async function handleCreateBackgroundAgent(request: CreateBackgroundAgentRequest) {
  const created = await store.createBackgroundAgent(request)
  if (created) {
    toast.success(`Background agent "${created.name}" created`)
  } else {
    toast.error('Failed to create background agent')
  }
  showCreateDialog.value = false
  await store.fetchBackgroundAgents()
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Content Area -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Loading state -->
      <div
        v-if="isLoading"
        class="flex flex-col items-center justify-center h-full text-muted-foreground"
      >
        <Loader2 :size="32" class="mb-2 animate-spin" />
        <span class="text-sm">Loading background agents...</span>
      </div>

      <!-- Grid View -->
      <div
        v-else-if="viewMode === 'grid'"
        class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4"
      >
        <!-- Existing background agents -->
        <HoverCard
          v-for="backgroundAgent in filteredBackgroundAgents"
          :key="backgroundAgent.id"
          :open-delay="500"
          :close-delay="100"
        >
          <HoverCardTrigger as-child>
            <Card
              class="group relative cursor-pointer hover:border-primary transition-colors"
              :class="{ 'opacity-50': loadingBackgroundAgentIds.has(backgroundAgent.id) }"
              @click="handleBackgroundAgentClick(backgroundAgent)"
            >
              <CardContent class="flex flex-col items-center justify-center p-6">
                <!-- Status indicator -->
                <div class="absolute top-2 left-2">
                  <span
                    class="h-2 w-2 rounded-full inline-block"
                    :class="getStatusIndicatorClass(backgroundAgent.status)"
                    :title="formatBackgroundAgentStatus(backgroundAgent.status)"
                  />
                </div>
                <!-- Icon -->
                <Loader2
                  v-if="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  :size="32"
                  class="text-muted-foreground mb-2 animate-spin"
                />
                <CalendarClock v-else :size="32" class="text-muted-foreground mb-2" />
                <span class="text-sm font-medium truncate w-full text-center">{{
                  backgroundAgent.name
                }}</span>
                <Badge :variant="getStatusVariant(backgroundAgent.status)" class="mt-1 text-xs">
                  {{ formatBackgroundAgentStatus(backgroundAgent.status) }}
                </Badge>
                <span class="text-xs text-muted-foreground mt-1">{{
                  formatSchedule(backgroundAgent.schedule)
                }}</span>
              </CardContent>
              <!-- Action buttons (show on hover) -->
              <div
                class="absolute top-1 right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <Button
                  v-if="backgroundAgent.status === 'active'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-orange-500"
                  title="Pause background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handlePauseBackgroundAgent($event, backgroundAgent)"
                >
                  <Pause :size="14" />
                </Button>
                <Button
                  v-if="backgroundAgent.status === 'paused'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-green-500"
                  title="Resume background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handleResumeBackgroundAgent($event, backgroundAgent)"
                >
                  <Play :size="14" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-destructive"
                  title="Delete background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handleDeleteBackgroundAgent($event, backgroundAgent)"
                >
                  <Trash2 :size="14" />
                </Button>
              </div>
            </Card>
          </HoverCardTrigger>

          <HoverCardContent class="w-72 p-0" side="right" :side-offset="8">
            <!-- Header -->
            <div class="px-3 py-2 border-b flex items-center gap-2">
              <CalendarClock :size="16" class="text-muted-foreground" />
              <span class="font-medium text-sm truncate">{{ backgroundAgent.name }}</span>
            </div>

            <!-- Description -->
            <div
              v-if="backgroundAgent.description"
              class="px-3 py-2 border-b text-sm text-muted-foreground"
            >
              {{ backgroundAgent.description }}
            </div>

            <!-- Schedule Info -->
            <div class="px-3 py-2 text-xs text-muted-foreground space-y-1">
              <div><strong>Schedule:</strong> {{ formatSchedule(backgroundAgent.schedule) }}</div>
              <div>
                <strong>Last run:</strong> {{ formatRelativeTime(backgroundAgent.last_run_at) }}
              </div>
              <div v-if="backgroundAgent.next_run_at">
                <strong>Next run:</strong> {{ formatRelativeTime(backgroundAgent.next_run_at) }}
              </div>
              <div class="flex gap-4 mt-2">
                <span class="text-green-600">{{ backgroundAgent.success_count }} success</span>
                <span class="text-red-600">{{ backgroundAgent.failure_count }} failed</span>
              </div>
            </div>

            <!-- Error -->
            <div v-if="backgroundAgent.last_error" class="px-3 py-2 border-t text-xs text-red-500">
              <strong>Error:</strong> {{ backgroundAgent.last_error }}
            </div>
          </HoverCardContent>
        </HoverCard>

        <!-- Create new background agent card -->
        <Card
          class="cursor-pointer border-dashed hover:border-primary transition-colors"
          @click="showCreateDialog = true"
        >
          <CardContent
            class="flex flex-col items-center justify-center p-6 text-muted-foreground hover:text-foreground transition-colors"
          >
            <Plus :size="32" class="mb-2" />
            <span class="text-sm">New Background Agent</span>
          </CardContent>
        </Card>
      </div>

      <!-- List View -->
      <div v-else-if="viewMode === 'list'" class="space-y-1">
        <HoverCard
          v-for="backgroundAgent in filteredBackgroundAgents"
          :key="backgroundAgent.id"
          :open-delay="500"
          :close-delay="100"
        >
          <HoverCardTrigger as-child>
            <button
              class="group w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left hover:bg-muted"
              :class="{ 'opacity-50': loadingBackgroundAgentIds.has(backgroundAgent.id) }"
              @click="handleBackgroundAgentClick(backgroundAgent)"
            >
              <!-- Status indicator -->
              <span
                class="h-2 w-2 rounded-full inline-block shrink-0"
                :class="getStatusIndicatorClass(backgroundAgent.status)"
                :title="formatBackgroundAgentStatus(backgroundAgent.status)"
              />

              <!-- Icon -->
              <Loader2
                v-if="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                :size="20"
                class="text-muted-foreground shrink-0 animate-spin"
              />
              <CalendarClock v-else :size="20" class="text-muted-foreground shrink-0" />

              <span class="flex-1 text-sm truncate">{{ backgroundAgent.name }}</span>

              <Badge :variant="getStatusVariant(backgroundAgent.status)" class="text-xs shrink-0">
                {{ formatBackgroundAgentStatus(backgroundAgent.status) }}
              </Badge>

              <span class="text-xs text-muted-foreground shrink-0">{{
                formatSchedule(backgroundAgent.schedule)
              }}</span>

              <!-- Action buttons (show on hover) -->
              <div
                class="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
              >
                <Button
                  v-if="backgroundAgent.status === 'active'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-orange-500"
                  title="Pause background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handlePauseBackgroundAgent($event, backgroundAgent)"
                >
                  <Pause :size="14" />
                </Button>
                <Button
                  v-if="backgroundAgent.status === 'paused'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-green-500"
                  title="Resume background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handleResumeBackgroundAgent($event, backgroundAgent)"
                >
                  <Play :size="14" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-destructive"
                  title="Delete background agent"
                  :disabled="loadingBackgroundAgentIds.has(backgroundAgent.id)"
                  @click="handleDeleteBackgroundAgent($event, backgroundAgent)"
                >
                  <Trash2 :size="14" />
                </Button>
              </div>
            </button>
          </HoverCardTrigger>

          <HoverCardContent class="w-72 p-0" side="right" :side-offset="8">
            <!-- Header -->
            <div class="px-3 py-2 border-b flex items-center gap-2">
              <CalendarClock :size="16" class="text-muted-foreground" />
              <span class="font-medium text-sm truncate">{{ backgroundAgent.name }}</span>
            </div>

            <!-- Description -->
            <div
              v-if="backgroundAgent.description"
              class="px-3 py-2 border-b text-sm text-muted-foreground"
            >
              {{ backgroundAgent.description }}
            </div>

            <!-- Schedule Info -->
            <div class="px-3 py-2 text-xs text-muted-foreground space-y-1">
              <div><strong>Schedule:</strong> {{ formatSchedule(backgroundAgent.schedule) }}</div>
              <div>
                <strong>Last run:</strong> {{ formatRelativeTime(backgroundAgent.last_run_at) }}
              </div>
              <div v-if="backgroundAgent.next_run_at">
                <strong>Next run:</strong> {{ formatRelativeTime(backgroundAgent.next_run_at) }}
              </div>
              <div class="flex gap-4 mt-2">
                <span class="text-green-600">{{ backgroundAgent.success_count }} success</span>
                <span class="text-red-600">{{ backgroundAgent.failure_count }} failed</span>
              </div>
            </div>

            <!-- Error -->
            <div v-if="backgroundAgent.last_error" class="px-3 py-2 border-t text-xs text-red-500">
              <strong>Error:</strong> {{ backgroundAgent.last_error }}
            </div>
          </HoverCardContent>
        </HoverCard>

        <!-- Create new background agent row -->
        <button
          class="w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left border-2 border-dashed hover:border-primary hover:bg-muted/50"
          @click="showCreateDialog = true"
        >
          <Plus :size="20" class="text-muted-foreground shrink-0" />
          <span class="flex-1 text-sm text-muted-foreground">New Background Agent</span>
        </button>
      </div>
    </div>

    <!-- Create Background Agent Dialog -->
    <CreateBackgroundAgentDialog
      v-model:open="showCreateDialog"
      @create="handleCreateBackgroundAgent"
    />
  </div>
</template>
