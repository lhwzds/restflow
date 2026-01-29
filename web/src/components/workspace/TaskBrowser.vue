<!--
  TaskBrowser Component - Design Decisions:

  1. Follows the same pattern as TerminalBrowser for consistency
  2. searchQuery and viewMode are PROPS, managed in parent (SkillWorkspace)
  3. Uses existing TaskCard component for display
  4. Uses CreateTaskDialog for creating new tasks
  5. Integrates with agentTaskStore for state management
-->
<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import {
  CalendarClock,
  Plus,
  Trash2,
  Loader2,
  Play,
  Pause,
} from 'lucide-vue-next'
import { storeToRefs } from 'pinia'
import { useAgentTaskStore } from '@/stores/agentTaskStore'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'
import { CreateTaskDialog } from '@/components/agent-task'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import { formatSchedule, formatTaskStatus } from '@/api/agent-task'

const props = defineProps<{
  searchQuery: string
  viewMode: 'grid' | 'list'
}>()

const store = useAgentTaskStore()
const { tasks, isLoading } = storeToRefs(store)
const toast = useToast()
const { confirm } = useConfirm()

// Local state
const showCreateDialog = ref(false)
const loadingTaskIds = ref<Set<string>>(new Set())

// Load tasks on mount
onMounted(() => {
  store.fetchTasks()
})

// Filter tasks by search query
const filteredTasks = computed(() => {
  if (!props.searchQuery) return tasks.value
  const query = props.searchQuery.toLowerCase()
  return tasks.value.filter(
    (task) =>
      task.name.toLowerCase().includes(query) ||
      task.description?.toLowerCase().includes(query)
  )
})

// Get status badge variant
function getStatusVariant(
  status: AgentTaskStatus
): 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'info' {
  const variantMap: Record<AgentTaskStatus, 'success' | 'info' | 'default' | 'destructive'> = {
    active: 'success',
    paused: 'info',
    running: 'default',
    completed: 'success',
    failed: 'destructive',
  }
  return variantMap[status] || 'default'
}

// Get status indicator class
function getStatusIndicatorClass(status: AgentTaskStatus): string {
  const classMap: Record<AgentTaskStatus, string> = {
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

// Handle task actions
async function handlePauseTask(event: Event, task: AgentTask) {
  event.stopPropagation()
  if (loadingTaskIds.value.has(task.id)) return

  loadingTaskIds.value.add(task.id)
  try {
    await store.pauseTask(task.id)
    toast.success(`Task "${task.name}" paused`)
  } catch (error) {
    toast.error('Failed to pause task')
  } finally {
    loadingTaskIds.value.delete(task.id)
  }
}

async function handleResumeTask(event: Event, task: AgentTask) {
  event.stopPropagation()
  if (loadingTaskIds.value.has(task.id)) return

  loadingTaskIds.value.add(task.id)
  try {
    await store.resumeTask(task.id)
    toast.success(`Task "${task.name}" resumed`)
  } catch (error) {
    toast.error('Failed to resume task')
  } finally {
    loadingTaskIds.value.delete(task.id)
  }
}

async function handleDeleteTask(event: Event, task: AgentTask) {
  event.stopPropagation()
  if (loadingTaskIds.value.has(task.id)) return

  const confirmed = await confirm({
    title: 'Delete Task',
    description: `Are you sure you want to delete "${task.name}"? This action cannot be undone.`,
    confirmText: 'Delete',
    cancelText: 'Cancel',
    variant: 'destructive',
  })

  if (!confirmed) return

  loadingTaskIds.value.add(task.id)
  try {
    await store.deleteTask(task.id)
    toast.success(`Task "${task.name}" deleted`)
  } catch (error) {
    toast.error('Failed to delete task')
  } finally {
    loadingTaskIds.value.delete(task.id)
  }
}

// Handle task click (could open details in future)
function handleTaskClick(task: AgentTask) {
  // TODO: Open task details in editor panel or dialog
  console.log('Task clicked:', task.id)
}

// Handle task created
function handleTaskCreated() {
  showCreateDialog.value = false
  store.fetchTasks()
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
        <span class="text-sm">Loading tasks...</span>
      </div>

      <!-- Grid View -->
      <div
        v-else-if="viewMode === 'grid'"
        class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4"
      >
        <!-- Existing tasks -->
        <HoverCard
          v-for="task in filteredTasks"
          :key="task.id"
          :open-delay="500"
          :close-delay="100"
        >
          <HoverCardTrigger as-child>
            <Card
              class="group relative cursor-pointer hover:border-primary transition-colors"
              :class="{ 'opacity-50': loadingTaskIds.has(task.id) }"
              @click="handleTaskClick(task)"
            >
              <CardContent class="flex flex-col items-center justify-center p-6">
                <!-- Status indicator -->
                <div class="absolute top-2 left-2">
                  <span
                    class="h-2 w-2 rounded-full inline-block"
                    :class="getStatusIndicatorClass(task.status)"
                    :title="formatTaskStatus(task.status)"
                  />
                </div>
                <!-- Icon -->
                <Loader2
                  v-if="loadingTaskIds.has(task.id)"
                  :size="32"
                  class="text-muted-foreground mb-2 animate-spin"
                />
                <CalendarClock v-else :size="32" class="text-muted-foreground mb-2" />
                <span class="text-sm font-medium truncate w-full text-center">{{
                  task.name
                }}</span>
                <Badge :variant="getStatusVariant(task.status)" class="mt-1 text-xs">
                  {{ formatTaskStatus(task.status) }}
                </Badge>
                <span class="text-xs text-muted-foreground mt-1">{{
                  formatSchedule(task.schedule)
                }}</span>
              </CardContent>
              <!-- Action buttons (show on hover) -->
              <div
                class="absolute top-1 right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <Button
                  v-if="task.status === 'active'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-orange-500"
                  title="Pause task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handlePauseTask($event, task)"
                >
                  <Pause :size="14" />
                </Button>
                <Button
                  v-if="task.status === 'paused'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-green-500"
                  title="Resume task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handleResumeTask($event, task)"
                >
                  <Play :size="14" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-destructive"
                  title="Delete task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handleDeleteTask($event, task)"
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
              <span class="font-medium text-sm truncate">{{ task.name }}</span>
            </div>

            <!-- Description -->
            <div v-if="task.description" class="px-3 py-2 border-b text-sm text-muted-foreground">
              {{ task.description }}
            </div>

            <!-- Schedule Info -->
            <div class="px-3 py-2 text-xs text-muted-foreground space-y-1">
              <div><strong>Schedule:</strong> {{ formatSchedule(task.schedule) }}</div>
              <div><strong>Last run:</strong> {{ formatRelativeTime(task.last_run_at) }}</div>
              <div v-if="task.next_run_at">
                <strong>Next run:</strong> {{ formatRelativeTime(task.next_run_at) }}
              </div>
              <div class="flex gap-4 mt-2">
                <span class="text-green-600">{{ task.success_count }} success</span>
                <span class="text-red-600">{{ task.failure_count }} failed</span>
              </div>
            </div>

            <!-- Error -->
            <div v-if="task.last_error" class="px-3 py-2 border-t text-xs text-red-500">
              <strong>Error:</strong> {{ task.last_error }}
            </div>
          </HoverCardContent>
        </HoverCard>

        <!-- Create new task card -->
        <Card
          class="cursor-pointer border-dashed hover:border-primary transition-colors"
          @click="showCreateDialog = true"
        >
          <CardContent
            class="flex flex-col items-center justify-center p-6 text-muted-foreground hover:text-foreground transition-colors"
          >
            <Plus :size="32" class="mb-2" />
            <span class="text-sm">New Task</span>
          </CardContent>
        </Card>
      </div>

      <!-- List View -->
      <div v-else-if="viewMode === 'list'" class="space-y-1">
        <HoverCard
          v-for="task in filteredTasks"
          :key="task.id"
          :open-delay="500"
          :close-delay="100"
        >
          <HoverCardTrigger as-child>
            <button
              class="group w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left hover:bg-muted"
              :class="{ 'opacity-50': loadingTaskIds.has(task.id) }"
              @click="handleTaskClick(task)"
            >
              <!-- Status indicator -->
              <span
                class="h-2 w-2 rounded-full inline-block shrink-0"
                :class="getStatusIndicatorClass(task.status)"
                :title="formatTaskStatus(task.status)"
              />

              <!-- Icon -->
              <Loader2
                v-if="loadingTaskIds.has(task.id)"
                :size="20"
                class="text-muted-foreground shrink-0 animate-spin"
              />
              <CalendarClock v-else :size="20" class="text-muted-foreground shrink-0" />

              <span class="flex-1 text-sm truncate">{{ task.name }}</span>

              <Badge :variant="getStatusVariant(task.status)" class="text-xs shrink-0">
                {{ formatTaskStatus(task.status) }}
              </Badge>

              <span class="text-xs text-muted-foreground shrink-0">{{
                formatSchedule(task.schedule)
              }}</span>

              <!-- Action buttons (show on hover) -->
              <div
                class="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
              >
                <Button
                  v-if="task.status === 'active'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-orange-500"
                  title="Pause task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handlePauseTask($event, task)"
                >
                  <Pause :size="14" />
                </Button>
                <Button
                  v-if="task.status === 'paused'"
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-green-500"
                  title="Resume task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handleResumeTask($event, task)"
                >
                  <Play :size="14" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6 text-muted-foreground hover:text-destructive"
                  title="Delete task"
                  :disabled="loadingTaskIds.has(task.id)"
                  @click="handleDeleteTask($event, task)"
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
              <span class="font-medium text-sm truncate">{{ task.name }}</span>
            </div>

            <!-- Description -->
            <div v-if="task.description" class="px-3 py-2 border-b text-sm text-muted-foreground">
              {{ task.description }}
            </div>

            <!-- Schedule Info -->
            <div class="px-3 py-2 text-xs text-muted-foreground space-y-1">
              <div><strong>Schedule:</strong> {{ formatSchedule(task.schedule) }}</div>
              <div><strong>Last run:</strong> {{ formatRelativeTime(task.last_run_at) }}</div>
              <div v-if="task.next_run_at">
                <strong>Next run:</strong> {{ formatRelativeTime(task.next_run_at) }}
              </div>
              <div class="flex gap-4 mt-2">
                <span class="text-green-600">{{ task.success_count }} success</span>
                <span class="text-red-600">{{ task.failure_count }} failed</span>
              </div>
            </div>

            <!-- Error -->
            <div v-if="task.last_error" class="px-3 py-2 border-t text-xs text-red-500">
              <strong>Error:</strong> {{ task.last_error }}
            </div>
          </HoverCardContent>
        </HoverCard>

        <!-- Create new task row -->
        <button
          class="w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left border-2 border-dashed hover:border-primary hover:bg-muted/50"
          @click="showCreateDialog = true"
        >
          <Plus :size="20" class="text-muted-foreground shrink-0" />
          <span class="flex-1 text-sm text-muted-foreground">New Task</span>
        </button>
      </div>
    </div>

    <!-- Create Task Dialog -->
    <CreateTaskDialog
      v-model:open="showCreateDialog"
      @created="handleTaskCreated"
    />
  </div>
</template>
