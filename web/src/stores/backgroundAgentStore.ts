/**
 * Background Agent Store
 *
 * Pinia store for managing background agent state, including CRUD operations,
 * filtering, sorting, and real-time status updates.
 */

import { defineStore } from 'pinia'
import type {
  BackgroundAgent,
  BackgroundAgentEvent,
  BackgroundAgentStatus,
  BackgroundAgentStreamEvent,
} from '@/types/background-agent'
import * as backgroundAgentApi from '@/api/background-agent'
import type {
  CreateBackgroundAgentRequest,
  UpdateBackgroundAgentRequest,
} from '@/api/background-agent'

export type SortField = 'name' | 'status' | 'created_at' | 'next_run_at' | 'last_run_at'
export type SortOrder = 'asc' | 'desc'

interface BackgroundAgentState {
  /** All loaded background agents */
  agents: BackgroundAgent[]
  /** Currently selected background agent ID */
  selectedBackgroundAgentId: string | null
  /** Events for the selected background agent */
  selectedBackgroundAgentEvents: BackgroundAgentEvent[]
  /** Loading state for background agent list */
  isLoading: boolean
  /** Loading state for background agent events */
  isLoadingEvents: boolean
  /** Error message if any */
  error: string | null
  /** Current filter by status */
  statusFilter: BackgroundAgentStatus | 'all'
  /** Current sort field */
  sortField: SortField
  /** Current sort order */
  sortOrder: SortOrder
  /** Search query for filtering background agents */
  searchQuery: string
  /** Version for reactive updates */
  version: number
  /** Timer handle for polling-based realtime sync */
  realtimeSyncTimer: ReturnType<typeof setInterval> | null
  /** Unlisten handler for background agent stream events */
  backgroundAgentStreamUnlisten: (() => void) | null
}

export const useBackgroundAgentStore = defineStore('backgroundAgent', {
  state: (): BackgroundAgentState => ({
    agents: [],
    selectedBackgroundAgentId: null,
    selectedBackgroundAgentEvents: [],
    isLoading: false,
    isLoadingEvents: false,
    error: null,
    statusFilter: 'all',
    sortField: 'created_at',
    sortOrder: 'desc',
    searchQuery: '',
    version: 0,
    realtimeSyncTimer: null,
    backgroundAgentStreamUnlisten: null,
  }),

  getters: {
    /**
     * Get filtered and sorted background agents
     */
    filteredBackgroundAgents(): BackgroundAgent[] {
      let result = [...this.agents]

      if (this.statusFilter !== 'all') {
        result = result.filter((backgroundAgent) => backgroundAgent.status === this.statusFilter)
      }

      if (this.searchQuery.trim()) {
        const query = this.searchQuery.toLowerCase()
        result = result.filter(
          (backgroundAgent) =>
            backgroundAgent.name.toLowerCase().includes(query) ||
            (backgroundAgent.description?.toLowerCase().includes(query) ?? false),
        )
      }

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
     * Get the currently selected background agent
     */
    selectedBackgroundAgent(): BackgroundAgent | null {
      if (!this.selectedBackgroundAgentId) return null
      return (
        this.agents.find(
          (backgroundAgent) => backgroundAgent.id === this.selectedBackgroundAgentId,
        ) ?? null
      )
    },

    /**
     * Get count of background agents by status
     */
    statusCounts(): Record<BackgroundAgentStatus | 'all', number> {
      const counts: Record<BackgroundAgentStatus | 'all', number> = {
        all: this.agents.length,
        active: 0,
        paused: 0,
        running: 0,
        completed: 0,
        failed: 0,
      }
      this.agents.forEach((backgroundAgent) => {
        counts[backgroundAgent.status]++
      })
      return counts
    },

    /**
     * Check if there are any background agents
     */
    hasBackgroundAgents(): boolean {
      return this.agents.length > 0
    },

    /**
     * Get active background agents (scheduled to run)
     */
    activeBackgroundAgents(): BackgroundAgent[] {
      return this.agents.filter(
        (backgroundAgent) =>
          backgroundAgent.status === 'active' || backgroundAgent.status === 'running',
      )
    },

    /**
     * Get background agents that need attention (failed or paused)
     */
    backgroundAgentsNeedingAttention(): BackgroundAgent[] {
      return this.agents.filter(
        (backgroundAgent) =>
          backgroundAgent.status === 'failed' || backgroundAgent.status === 'paused',
      )
    },
  },

  actions: {
    /**
     * Fetch all background agents from the API
     */
    async fetchBackgroundAgents(): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        this.agents = await backgroundAgentApi.listBackgroundAgents()
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch background agents'
        console.error('Failed to fetch background agents:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Fetch background agents filtered by status
     */
    async fetchBackgroundAgentsByStatus(status: BackgroundAgentStatus): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        const agentsByStatus = await backgroundAgentApi.listBackgroundAgentsByStatus(status)
        const backgroundAgentIds = new Set(
          agentsByStatus.map((backgroundAgent) => backgroundAgent.id),
        )
        this.agents = [
          ...this.agents.filter((backgroundAgent) => !backgroundAgentIds.has(backgroundAgent.id)),
          ...agentsByStatus,
        ]
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch background agents'
        console.error('Failed to fetch background agents by status:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Get a single background agent by ID
     */
    async getBackgroundAgent(id: string): Promise<BackgroundAgent | null> {
      try {
        const backgroundAgent = await backgroundAgentApi.getBackgroundAgent(id)
        const index = this.agents.findIndex((agent) => agent.id === id)
        if (index >= 0) {
          this.agents[index] = backgroundAgent
        } else {
          this.agents.push(backgroundAgent)
        }
        this.version++
        return backgroundAgent
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to get background agent'
        console.error('Failed to get background agent:', err)
        return null
      }
    },

    /**
     * Create a new background agent
     */
    async createBackgroundAgent(
      request: CreateBackgroundAgentRequest,
    ): Promise<BackgroundAgent | null> {
      this.error = null
      try {
        const backgroundAgent = await backgroundAgentApi.createBackgroundAgent(request)
        this.agents.push(backgroundAgent)
        this.version++
        return backgroundAgent
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to create background agent'
        console.error('Failed to create background agent:', err)
        return null
      }
    },

    /**
     * Update an existing background agent
     */
    async updateBackgroundAgent(
      id: string,
      request: UpdateBackgroundAgentRequest,
    ): Promise<BackgroundAgent | null> {
      this.error = null
      try {
        const backgroundAgent = await backgroundAgentApi.updateBackgroundAgent(id, request)
        const index = this.agents.findIndex((agent) => agent.id === id)
        if (index >= 0) {
          this.agents[index] = backgroundAgent
        }
        this.version++
        return backgroundAgent
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to update background agent'
        console.error('Failed to update background agent:', err)
        return null
      }
    },

    /**
     * Delete a background agent
     */
    async deleteBackgroundAgent(id: string): Promise<boolean> {
      this.error = null
      try {
        const success = await backgroundAgentApi.deleteBackgroundAgent(id)
        if (success) {
          this.agents = this.agents.filter((backgroundAgent) => backgroundAgent.id !== id)
          if (this.selectedBackgroundAgentId === id) {
            this.selectedBackgroundAgentId = null
            this.selectedBackgroundAgentEvents = []
          }
          this.version++
        }
        return success
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to delete background agent'
        console.error('Failed to delete background agent:', err)
        return false
      }
    },

    /**
     * Pause a background agent
     */
    async pauseBackgroundAgent(id: string): Promise<boolean> {
      this.error = null
      try {
        const backgroundAgent = await backgroundAgentApi.pauseBackgroundAgent(id)
        const index = this.agents.findIndex((agent) => agent.id === id)
        if (index >= 0) {
          this.agents[index] = backgroundAgent
        }
        this.version++
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to pause background agent'
        console.error('Failed to pause background agent:', err)
        return false
      }
    },

    /**
     * Resume a paused background agent
     */
    async resumeBackgroundAgent(id: string): Promise<boolean> {
      this.error = null
      try {
        const backgroundAgent = await backgroundAgentApi.resumeBackgroundAgent(id)
        const index = this.agents.findIndex((agent) => agent.id === id)
        if (index >= 0) {
          this.agents[index] = backgroundAgent
        }
        this.version++
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to resume background agent'
        console.error('Failed to resume background agent:', err)
        return false
      }
    },

    /**
     * Fetch events for a background agent
     */
    async fetchBackgroundAgentEvents(backgroundAgentId: string, limit?: number): Promise<void> {
      this.isLoadingEvents = true
      try {
        this.selectedBackgroundAgentEvents = await backgroundAgentApi.getBackgroundAgentEvents(
          backgroundAgentId,
          limit,
        )
      } catch (err) {
        console.error('Failed to fetch background agent events:', err)
        this.selectedBackgroundAgentEvents = []
      } finally {
        this.isLoadingEvents = false
      }
    },

    /**
     * Select a background agent and load its events
     */
    async selectBackgroundAgent(backgroundAgentId: string | null): Promise<void> {
      this.selectedBackgroundAgentId = backgroundAgentId
      if (backgroundAgentId) {
        await this.fetchBackgroundAgentEvents(backgroundAgentId)
      } else {
        this.selectedBackgroundAgentEvents = []
      }
    },

    /**
     * Set status filter
     */
    setStatusFilter(status: BackgroundAgentStatus | 'all'): void {
      this.statusFilter = status
    },

    /**
     * Set sort options
     */
    setSort(field: SortField, order?: SortOrder): void {
      if (this.sortField === field && !order) {
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
     * Update a background agent in local state (for real-time updates)
     */
    updateBackgroundAgentLocally(backgroundAgent: BackgroundAgent): void {
      const index = this.agents.findIndex((agent) => agent.id === backgroundAgent.id)
      if (index >= 0) {
        this.agents[index] = backgroundAgent
      } else {
        this.agents.push(backgroundAgent)
      }
      this.version++
    },

    /**
     * Remove a background agent from local state
     */
    removeBackgroundAgentLocally(backgroundAgentId: string): void {
      this.agents = this.agents.filter(
        (backgroundAgent) => backgroundAgent.id !== backgroundAgentId,
      )
      if (this.selectedBackgroundAgentId === backgroundAgentId) {
        this.selectedBackgroundAgentId = null
        this.selectedBackgroundAgentEvents = []
      }
      this.version++
    },

    /**
     * Start realtime background agent synchronization.
     *
     * Uses event-stream updates when available (Tauri) and falls back to polling.
     */
    async startRealtimeSync(intervalMs = 3000): Promise<void> {
      if (!this.backgroundAgentStreamUnlisten) {
        this.backgroundAgentStreamUnlisten = await backgroundAgentApi.onBackgroundAgentStreamEvent(
          (event: BackgroundAgentStreamEvent) => {
            const type = event.kind.type
            if (
              type === 'started' ||
              type === 'completed' ||
              type === 'failed' ||
              type === 'cancelled'
            ) {
              void this.getBackgroundAgent(event.task_id)
            }
          },
        )
      }

      if (!this.realtimeSyncTimer) {
        this.realtimeSyncTimer = setInterval(() => {
          void this.fetchBackgroundAgents()
        }, intervalMs)
      }
    },

    /**
     * Stop realtime background agent synchronization.
     */
    stopRealtimeSync(): void {
      if (this.backgroundAgentStreamUnlisten) {
        this.backgroundAgentStreamUnlisten()
        this.backgroundAgentStreamUnlisten = null
      }

      if (this.realtimeSyncTimer) {
        clearInterval(this.realtimeSyncTimer)
        this.realtimeSyncTimer = null
      }
    },
  },
})
