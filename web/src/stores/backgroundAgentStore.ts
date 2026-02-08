/**
 * Agent Task Store
 *
 * Pinia store for managing agent tasks state, including CRUD operations,
 * filtering, sorting, and real-time status updates.
 */

import { defineStore } from 'pinia'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import type { TaskEvent } from '@/types/generated/TaskEvent'
import type { TaskStreamEvent } from '@/types/generated/TaskStreamEvent'
import * as backgroundAgentApi from '@/api/background-agent'
import type { CreateBackgroundAgentRequest, UpdateBackgroundAgentRequest } from '@/api/background-agent'

export type SortField = 'name' | 'status' | 'created_at' | 'next_run_at' | 'last_run_at'
export type SortOrder = 'asc' | 'desc'

interface AgentTaskState {
  /** All loaded tasks */
  tasks: AgentTask[]
  /** Currently selected task ID */
  selectedTaskId: string | null
  /** Events for the selected task */
  selectedTaskEvents: TaskEvent[]
  /** Loading state for task list */
  isLoading: boolean
  /** Loading state for task events */
  isLoadingEvents: boolean
  /** Error message if any */
  error: string | null
  /** Current filter by status */
  statusFilter: AgentTaskStatus | 'all'
  /** Current sort field */
  sortField: SortField
  /** Current sort order */
  sortOrder: SortOrder
  /** Search query for filtering tasks */
  searchQuery: string
  /** Version for reactive updates */
  version: number
  /** Timer handle for polling-based realtime sync */
  realtimeSyncTimer: ReturnType<typeof setInterval> | null
  /** Unlisten handler for task stream events */
  taskStreamUnlisten: (() => void) | null
}

export const useBackgroundAgentStore = defineStore('backgroundAgent', {
  state: (): AgentTaskState => ({
    tasks: [],
    selectedTaskId: null,
    selectedTaskEvents: [],
    isLoading: false,
    isLoadingEvents: false,
    error: null,
    statusFilter: 'all',
    sortField: 'created_at',
    sortOrder: 'desc',
    searchQuery: '',
    version: 0,
    realtimeSyncTimer: null,
    taskStreamUnlisten: null,
  }),

  getters: {
    /**
     * Get filtered and sorted tasks
     */
    filteredTasks(): AgentTask[] {
      let result = [...this.tasks]

      // Apply status filter
      if (this.statusFilter !== 'all') {
        result = result.filter((task) => task.status === this.statusFilter)
      }

      // Apply search filter
      if (this.searchQuery.trim()) {
        const query = this.searchQuery.toLowerCase()
        result = result.filter(
          (task) =>
            task.name.toLowerCase().includes(query) ||
            (task.description?.toLowerCase().includes(query) ?? false),
        )
      }

      // Apply sorting
      result.sort((a, b) => {
        let comparison = 0
        switch (this.sortField) {
          case 'name':
            comparison = a.name.localeCompare(b.name)
            break
          case 'status':
            comparison = a.status.localeCompare(b.status)
            break
          case 'created_at':
            comparison = a.created_at - b.created_at
            break
          case 'next_run_at':
            // Handle null values - push to end
            if (a.next_run_at === null && b.next_run_at === null) comparison = 0
            else if (a.next_run_at === null) comparison = 1
            else if (b.next_run_at === null) comparison = -1
            else comparison = a.next_run_at - b.next_run_at
            break
          case 'last_run_at':
            if (a.last_run_at === null && b.last_run_at === null) comparison = 0
            else if (a.last_run_at === null) comparison = 1
            else if (b.last_run_at === null) comparison = -1
            else comparison = a.last_run_at - b.last_run_at
            break
        }
        return this.sortOrder === 'asc' ? comparison : -comparison
      })

      return result
    },

    /**
     * Get the currently selected task
     */
    selectedTask(): AgentTask | null {
      if (!this.selectedTaskId) return null
      return this.tasks.find((t) => t.id === this.selectedTaskId) ?? null
    },

    /**
     * Get count of tasks by status
     */
    statusCounts(): Record<AgentTaskStatus | 'all', number> {
      const counts: Record<AgentTaskStatus | 'all', number> = {
        all: this.tasks.length,
        active: 0,
        paused: 0,
        running: 0,
        completed: 0,
        failed: 0,
      }
      this.tasks.forEach((task) => {
        counts[task.status]++
      })
      return counts
    },

    /**
     * Check if there are any tasks
     */
    hasTasks(): boolean {
      return this.tasks.length > 0
    },

    /**
     * Get active tasks (scheduled to run)
     */
    activeTasks(): AgentTask[] {
      return this.tasks.filter((t) => t.status === 'active' || t.status === 'running')
    },

    /**
     * Get tasks that need attention (failed or paused)
     */
    tasksNeedingAttention(): AgentTask[] {
      return this.tasks.filter((t) => t.status === 'failed' || t.status === 'paused')
    },
  },

  actions: {
    /**
     * Fetch all tasks from the API
     */
    async fetchTasks(): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        this.tasks = await backgroundAgentApi.listBackgroundAgents()
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch tasks'
        console.error('Failed to fetch agent tasks:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Fetch tasks filtered by status
     */
    async fetchTasksByStatus(status: AgentTaskStatus): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        const tasks = await backgroundAgentApi.listBackgroundAgentsByStatus(status)
        // Merge with existing tasks, replacing those with matching IDs
        const taskIds = new Set(tasks.map((t) => t.id))
        this.tasks = [...this.tasks.filter((t) => !taskIds.has(t.id)), ...tasks]
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch tasks'
        console.error('Failed to fetch agent tasks by status:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Get a single task by ID
     */
    async getTask(id: string): Promise<AgentTask | null> {
      try {
        const task = await backgroundAgentApi.getBackgroundAgent(id)
        // Update in local state
        const index = this.tasks.findIndex((t) => t.id === id)
        if (index >= 0) {
          this.tasks[index] = task
        } else {
          this.tasks.push(task)
        }
        this.version++
        return task
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to get task'
        console.error('Failed to get agent task:', err)
        return null
      }
    },

    /**
     * Create a new task
     */
    async createTask(request: CreateBackgroundAgentRequest): Promise<AgentTask | null> {
      this.error = null
      try {
        const task = await backgroundAgentApi.createBackgroundAgent(request)
        this.tasks.push(task)
        this.version++
        return task
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to create task'
        console.error('Failed to create agent task:', err)
        return null
      }
    },

    /**
     * Update an existing task
     */
    async updateTask(id: string, request: UpdateBackgroundAgentRequest): Promise<AgentTask | null> {
      this.error = null
      try {
        const task = await backgroundAgentApi.updateBackgroundAgent(id, request)
        const index = this.tasks.findIndex((t) => t.id === id)
        if (index >= 0) {
          this.tasks[index] = task
        }
        this.version++
        return task
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to update task'
        console.error('Failed to update agent task:', err)
        return null
      }
    },

    /**
     * Delete a task
     */
    async deleteTask(id: string): Promise<boolean> {
      this.error = null
      try {
        const success = await backgroundAgentApi.deleteBackgroundAgent(id)
        if (success) {
          this.tasks = this.tasks.filter((t) => t.id !== id)
          if (this.selectedTaskId === id) {
            this.selectedTaskId = null
            this.selectedTaskEvents = []
          }
          this.version++
        }
        return success
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to delete task'
        console.error('Failed to delete agent task:', err)
        return false
      }
    },

    /**
     * Pause a task
     */
    async pauseTask(id: string): Promise<boolean> {
      this.error = null
      try {
        const task = await backgroundAgentApi.pauseBackgroundAgent(id)
        const index = this.tasks.findIndex((t) => t.id === id)
        if (index >= 0) {
          this.tasks[index] = task
        }
        this.version++
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to pause task'
        console.error('Failed to pause agent task:', err)
        return false
      }
    },

    /**
     * Resume a paused task
     */
    async resumeTask(id: string): Promise<boolean> {
      this.error = null
      try {
        const task = await backgroundAgentApi.resumeBackgroundAgent(id)
        const index = this.tasks.findIndex((t) => t.id === id)
        if (index >= 0) {
          this.tasks[index] = task
        }
        this.version++
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to resume task'
        console.error('Failed to resume agent task:', err)
        return false
      }
    },

    /**
     * Fetch events for a task
     */
    async fetchTaskEvents(taskId: string, limit?: number): Promise<void> {
      this.isLoadingEvents = true
      try {
        this.selectedTaskEvents = await backgroundAgentApi.getBackgroundAgentEvents(taskId, limit)
      } catch (err) {
        console.error('Failed to fetch task events:', err)
        this.selectedTaskEvents = []
      } finally {
        this.isLoadingEvents = false
      }
    },

    /**
     * Select a task and load its events
     */
    async selectTask(taskId: string | null): Promise<void> {
      this.selectedTaskId = taskId
      if (taskId) {
        await this.fetchTaskEvents(taskId)
      } else {
        this.selectedTaskEvents = []
      }
    },

    /**
     * Set status filter
     */
    setStatusFilter(status: AgentTaskStatus | 'all'): void {
      this.statusFilter = status
    },

    /**
     * Set sort options
     */
    setSort(field: SortField, order?: SortOrder): void {
      if (this.sortField === field && !order) {
        // Toggle order if same field
        this.sortOrder = this.sortOrder === 'asc' ? 'desc' : 'asc'
      } else {
        this.sortField = field
        this.sortOrder = order ?? 'desc'
      }
    },

    /**
     * Set search query
     */
    setSearchQuery(query: string): void {
      this.searchQuery = query
    },

    /**
     * Clear all filters
     */
    clearFilters(): void {
      this.statusFilter = 'all'
      this.searchQuery = ''
      this.sortField = 'created_at'
      this.sortOrder = 'desc'
    },

    /**
     * Clear error
     */
    clearError(): void {
      this.error = null
    },

    /**
     * Update a task in the local state (for real-time updates)
     */
    updateTaskLocally(task: AgentTask): void {
      const index = this.tasks.findIndex((t) => t.id === task.id)
      if (index >= 0) {
        this.tasks[index] = task
      } else {
        this.tasks.push(task)
      }
      this.version++
    },

    /**
     * Remove a task from local state
     */
    removeTaskLocally(taskId: string): void {
      this.tasks = this.tasks.filter((t) => t.id !== taskId)
      if (this.selectedTaskId === taskId) {
        this.selectedTaskId = null
        this.selectedTaskEvents = []
      }
      this.version++
    },

    /**
     * Start realtime task synchronization.
     *
     * Uses event-stream updates when available (Tauri) and falls back to polling.
     */
    async startRealtimeSync(intervalMs = 3000): Promise<void> {
      if (!this.taskStreamUnlisten) {
        this.taskStreamUnlisten = await backgroundAgentApi.onBackgroundAgentStreamEvent((event: TaskStreamEvent) => {
          const type = event.kind.type
          if (
            type === 'started' ||
            type === 'completed' ||
            type === 'failed' ||
            type === 'cancelled'
          ) {
            void this.getTask(event.task_id)
          }
        })
      }

      if (!this.realtimeSyncTimer) {
        this.realtimeSyncTimer = setInterval(() => {
          void this.fetchTasks()
        }, intervalMs)
      }
    },

    /**
     * Stop realtime task synchronization.
     */
    stopRealtimeSync(): void {
      if (this.taskStreamUnlisten) {
        this.taskStreamUnlisten()
        this.taskStreamUnlisten = null
      }

      if (this.realtimeSyncTimer) {
        clearInterval(this.realtimeSyncTimer)
        this.realtimeSyncTimer = null
      }
    },
  },
})
