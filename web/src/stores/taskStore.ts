import { defineStore } from 'pinia'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'
import * as api from '@/api/task'

interface TaskState {
  agents: Task[]
  selectedAgentId: string | null
  isLoading: boolean
  error: string | null
  statusFilter: TaskStatus | null
}

export const useTaskStore = defineStore('task', {
  state: (): TaskState => ({
    agents: [],
    selectedAgentId: null,
    isLoading: false,
    error: null,
    statusFilter: null,
  }),

  getters: {
    tasks(): Task[] {
      return this.agents
    },

    filteredTasks(): Task[] {
      if (!this.statusFilter) return this.agents
      return this.agents.filter((a) => a.status === this.statusFilter)
    },

    selectedTask(): Task | null {
      if (!this.selectedAgentId) return null
      return this.agents.find((a) => a.id === this.selectedAgentId) ?? null
    },

    runningTaskCount(): number {
      return this.agents.filter((a) => a.status === 'running').length
    },

    /** Look up a task by its bound chat session ID. */
    taskBySessionId() {
      return (sessionId: string): Task | null =>
        this.agents.find((a) => a.chat_session_id === sessionId) ?? null
    },
  },

  actions: {
    async fetchTasks(): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        this.agents = await api.listTasks()
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch tasks'
        console.error('Failed to fetch tasks:', err)
      } finally {
        this.isLoading = false
      }
    },

    selectTask(id: string | null): void {
      this.selectedAgentId = id
    },

    setStatusFilter(status: TaskStatus | null): void {
      this.statusFilter = status
    },

    async pauseTask(id: string): Promise<void> {
      this.error = null
      try {
        const updated = await api.pauseTask(id)
        this.upsertTaskLocally(updated)
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to pause task'
        console.error('Failed to pause task:', err)
      }
    },

    async resumeTask(id: string): Promise<void> {
      this.error = null
      try {
        const updated = await api.resumeTask(id)
        this.upsertTaskLocally(updated)
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to resume task'
        console.error('Failed to resume task:', err)
      }
    },

    async stopTask(taskId: string): Promise<void> {
      this.error = null
      try {
        const updated = await api.stopTask(taskId)
        this.upsertTaskLocally(updated)
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to stop task'
        console.error('Failed to stop task:', err)
      }
    },

    async runTaskNow(id: string): Promise<Task | null> {
      this.error = null
      try {
        const task = await api.runTaskNow(id)
        this.upsertTaskLocally(task)
        return task
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to run task'
        console.error('Failed to run task:', err)
        return null
      }
    },

    async deleteTask(id: string): Promise<boolean> {
      this.error = null
      try {
        const result = await api.deleteTask(id)
        if (result.deleted) {
          this.agents = this.agents.filter((a) => a.id !== id)
          if (this.selectedAgentId === id) {
            this.selectedAgentId = null
          }
        }
        return result.deleted
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to delete task'
        console.error('Failed to delete task:', err)
        return false
      }
    },

    async convertSessionToTask(
      request: api.CreateTaskFromSessionRequest,
    ): Promise<Task | null> {
      this.error = null
      try {
        const result = await api.createTaskFromSession(request)
        this.upsertTaskLocally(result.task)
        return result.task
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to convert session'
        console.error('Failed to convert session to task:', err)
        return null
      }
    },

    /**
     * Convert a task-linked session back to a normal workspace session by
     * removing the task binding while keeping the chat session.
     */
    async convertTaskToWorkspace(sessionId: string): Promise<boolean> {
      this.error = null
      try {
        let target = this.agents.find((agent) => agent.chat_session_id === sessionId) ?? null
        if (!target) {
          await this.fetchTasks()
          target = this.agents.find((agent) => agent.chat_session_id === sessionId) ?? null
        }

        if (!target) {
          this.error = 'Task binding not found for this session'
          return false
        }

        const deleted = await this.deleteTask(target.id)
        if (!deleted) {
          return false
        }
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to convert session'
        console.error('Failed to convert task session to workspace session:', err)
        return false
      }
    },

    upsertTaskLocally(task: Task): void {
      const idx = this.agents.findIndex((a) => a.id === task.id)
      if (idx >= 0) {
        this.agents[idx] = task
      } else {
        this.agents.push(task)
      }
    },

  },
})
