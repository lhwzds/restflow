<script setup lang="ts">
/**
 * AgentTaskList View
 *
 * Main view for managing agent tasks. Displays a list of scheduled agent
 * tasks with filtering, search, and CRUD operations.
 */

import { ref, computed, onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import {
  Plus,
  Search,
  RefreshCw,
  LayoutGrid,
  List,
  Filter,
  Clock,
  ArrowUpDown,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import { TaskCard, CreateTaskDialog } from '@/components/agent-task'
import { useAgentTaskStore, type SortField } from '@/stores/agentTaskStore'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import type { CreateAgentTaskRequest } from '@/api/agent-task'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'

const toast = useToast()
const { confirm } = useConfirm()
const store = useAgentTaskStore()

// Store state
const { filteredTasks, isLoading, error, statusFilter, statusCounts, searchQuery, sortField, sortOrder } =
  storeToRefs(store)

// Local state
const viewMode = ref<'grid' | 'list'>('grid')
const showCreateDialog = ref(false)
const loadingTaskId = ref<string | null>(null)

// Status filter options
const statusOptions: Array<{ value: AgentTaskStatus | 'all'; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'active', label: 'Active' },
  { value: 'paused', label: 'Paused' },
  { value: 'running', label: 'Running' },
  { value: 'completed', label: 'Completed' },
  { value: 'failed', label: 'Failed' },
]

// Sort options
const sortOptions: Array<{ value: SortField; label: string }> = [
  { value: 'created_at', label: 'Created' },
  { value: 'name', label: 'Name' },
  { value: 'status', label: 'Status' },
  { value: 'next_run_at', label: 'Next Run' },
  { value: 'last_run_at', label: 'Last Run' },
]

// Computed
const currentStatusLabel = computed(() => {
  const option = statusOptions.find((o) => o.value === statusFilter.value)
  return option?.label || 'All'
})

const currentSortLabel = computed(() => {
  const option = sortOptions.find((o) => o.value === sortField.value)
  return option?.label || 'Created'
})

const isEmpty = computed(() => filteredTasks.value.length === 0 && !isLoading.value)

// Load tasks on mount
onMounted(() => {
  store.fetchTasks()
})

// Handlers
async function handleRefresh() {
  await store.fetchTasks()
}

function handleSearch(event: Event) {
  const target = event.target as HTMLInputElement
  store.setSearchQuery(target.value)
}

function handleStatusFilter(value: string) {
  store.setStatusFilter(value as AgentTaskStatus | 'all')
}

function handleSort(field: SortField) {
  store.setSort(field)
}

function handleTaskClick(task: AgentTask) {
  store.selectTask(task.id)
  // TODO: Open task detail panel or dialog
}

async function handlePauseTask(task: AgentTask) {
  loadingTaskId.value = task.id
  try {
    const success = await store.pauseTask(task.id)
    if (success) {
      toast.success(`Task "${task.name}" paused`)
    }
  } catch (e) {
    toast.error('Failed to pause task')
  } finally {
    loadingTaskId.value = null
  }
}

async function handleResumeTask(task: AgentTask) {
  loadingTaskId.value = task.id
  try {
    const success = await store.resumeTask(task.id)
    if (success) {
      toast.success(`Task "${task.name}" resumed`)
    }
  } catch (e) {
    toast.error('Failed to resume task')
  } finally {
    loadingTaskId.value = null
  }
}

async function handleDeleteTask(task: AgentTask) {
  const confirmed = await confirm({
    title: 'Delete Task',
    description: `Are you sure you want to delete "${task.name}"? This action cannot be undone.`,
    confirmText: 'Delete',
    cancelText: 'Cancel',
    variant: 'destructive',
  })

  if (!confirmed) return

  loadingTaskId.value = task.id
  try {
    const success = await store.deleteTask(task.id)
    if (success) {
      toast.success(`Task "${task.name}" deleted`)
    }
  } catch (e) {
    toast.error('Failed to delete task')
  } finally {
    loadingTaskId.value = null
  }
}

async function handleCreateTask(request: CreateAgentTaskRequest) {
  try {
    const task = await store.createTask(request)
    if (task) {
      toast.success(`Task "${task.name}" created`)
    }
  } catch (e) {
    toast.error('Failed to create task')
  }
}
</script>

<template>
  <div class="agent-task-list">
    <!-- Header -->
    <header class="list-header">
      <div class="header-left">
        <h1 class="title">
          <Clock :size="24" />
          Agent Tasks
        </h1>
        <Badge variant="secondary" class="task-count">
          {{ filteredTasks.length }} tasks
        </Badge>
      </div>

      <div class="header-right">
        <!-- Search -->
        <div class="search-wrapper">
          <Search :size="14" class="search-icon" />
          <Input
            :model-value="searchQuery"
            placeholder="Search tasks..."
            class="search-input"
            @input="handleSearch"
          />
        </div>

        <!-- Status Filter -->
        <Select :model-value="statusFilter" @update:model-value="handleStatusFilter">
          <SelectTrigger class="filter-trigger">
            <Filter :size="14" />
            <SelectValue :placeholder="currentStatusLabel" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="option in statusOptions" :key="option.value" :value="option.value">
              {{ option.label }}
              <Badge v-if="option.value !== 'all'" variant="outline" class="ml-2">
                {{ statusCounts[option.value] }}
              </Badge>
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Sort -->
        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm" class="sort-btn">
              <ArrowUpDown :size="14" />
              {{ currentSortLabel }}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              v-for="option in sortOptions"
              :key="option.value"
              @click="handleSort(option.value)"
            >
              {{ option.label }}
              <span v-if="sortField === option.value" class="ml-auto text-muted-foreground">
                {{ sortOrder === 'asc' ? '↑' : '↓' }}
              </span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <!-- View Mode Toggle -->
        <div class="view-toggle">
          <Button
            variant="ghost"
            size="icon"
            :class="{ 'bg-muted': viewMode === 'list' }"
            @click="viewMode = 'list'"
          >
            <List :size="16" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            :class="{ 'bg-muted': viewMode === 'grid' }"
            @click="viewMode = 'grid'"
          >
            <LayoutGrid :size="16" />
          </Button>
        </div>

        <DropdownMenuSeparator class="h-6 w-px bg-border mx-1" />

        <!-- Refresh -->
        <Button variant="ghost" size="icon" :disabled="isLoading" @click="handleRefresh">
          <RefreshCw :size="16" :class="{ 'animate-spin': isLoading }" />
        </Button>

        <!-- Create -->
        <Button @click="showCreateDialog = true">
          <Plus :size="16" class="mr-1" />
          New Task
        </Button>
      </div>
    </header>

    <!-- Error Message -->
    <div v-if="error" class="error-banner">
      {{ error }}
      <Button variant="ghost" size="sm" @click="store.clearError">Dismiss</Button>
    </div>

    <!-- Content -->
    <main class="list-content">
      <!-- Loading State -->
      <div v-if="isLoading && filteredTasks.length === 0" class="loading-state">
        <RefreshCw :size="24" class="animate-spin text-muted-foreground" />
        <p>Loading tasks...</p>
      </div>

      <!-- Empty State -->
      <div v-else-if="isEmpty" class="empty-state">
        <Clock :size="48" class="empty-icon" />
        <h2 class="empty-title">No tasks found</h2>
        <p class="empty-description">
          {{
            searchQuery || statusFilter !== 'all'
              ? 'Try adjusting your filters'
              : 'Create your first scheduled agent task'
          }}
        </p>
        <Button v-if="!searchQuery && statusFilter === 'all'" @click="showCreateDialog = true">
          <Plus :size="16" class="mr-1" />
          Create Task
        </Button>
        <Button v-else variant="outline" @click="store.clearFilters"> Clear Filters </Button>
      </div>

      <!-- Task Grid -->
      <div v-else :class="['task-grid', `task-grid--${viewMode}`]">
        <TaskCard
          v-for="task in filteredTasks"
          :key="task.id"
          :task="task"
          :is-loading="loadingTaskId === task.id"
          @click="handleTaskClick"
          @pause="handlePauseTask"
          @resume="handleResumeTask"
          @delete="handleDeleteTask"
        />
      </div>
    </main>

    <!-- Create Dialog -->
    <CreateTaskDialog
      :open="showCreateDialog"
      @update:open="showCreateDialog = $event"
      @create="handleCreateTask"
    />
  </div>
</template>

<style lang="scss" scoped>
.agent-task-list {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--rf-color-bg-base);
}

.list-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-md) var(--rf-spacing-lg);
  border-bottom: 1px solid var(--rf-color-border);
  background: var(--rf-color-bg-container);

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);

    .title {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-sm);
      font-size: var(--rf-font-size-lg);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
    }

    .task-count {
      font-size: var(--rf-font-size-xs);
    }
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);

    .search-wrapper {
      position: relative;

      .search-icon {
        position: absolute;
        left: var(--rf-spacing-sm);
        top: 50%;
        transform: translateY(-50%);
        color: var(--rf-color-text-secondary);
      }

      .search-input {
        width: 200px;
        padding-left: 32px;
        height: 32px;
      }
    }

    .filter-trigger {
      width: 120px;
      height: 32px;
      gap: var(--rf-spacing-xs);
    }

    .sort-btn {
      gap: var(--rf-spacing-xs);
    }

    .view-toggle {
      display: flex;
      border: 1px solid var(--rf-color-border);
      border-radius: var(--rf-radius-sm);
      overflow: hidden;

      button {
        border-radius: 0;
        height: 32px;
        width: 32px;
      }
    }
  }
}

.error-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-sm) var(--rf-spacing-lg);
  background: var(--rf-color-danger-light);
  color: var(--rf-color-danger);
  font-size: var(--rf-font-size-sm);
}

.list-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--rf-spacing-lg);
}

.loading-state,
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: var(--rf-spacing-md);
  color: var(--rf-color-text-secondary);

  .empty-icon {
    opacity: 0.3;
  }

  .empty-title {
    font-size: var(--rf-font-size-lg);
    font-weight: var(--rf-font-weight-semibold);
    color: var(--rf-color-text-primary);
  }

  .empty-description {
    font-size: var(--rf-font-size-sm);
  }
}

.task-grid {
  display: grid;
  gap: var(--rf-spacing-md);

  &--grid {
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  }

  &--list {
    grid-template-columns: 1fr;

    :deep(.task-card) {
      height: auto;
      min-height: 120px;

      .card-body {
        flex-direction: row;
        flex-wrap: wrap;
        gap: var(--rf-spacing-md);
      }
    }
  }
}
</style>
