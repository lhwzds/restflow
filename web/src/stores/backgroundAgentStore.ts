/**
 * Background Agent Store
 *
 * Pinia store for managing background agents in the workspace.
 * Provides listing, selection, control actions, and status filtering.
 */

import { defineStore } from 'pinia'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { BackgroundAgentStatus } from '@/types/generated/BackgroundAgentStatus'
import * as api from '@/api/background-agents'

interface BackgroundAgentState {
  agents: BackgroundAgent[]
  selectedAgentId: string | null
  isLoading: boolean
  error: string | null
  statusFilter: BackgroundAgentStatus | null
}

export const useBackgroundAgentStore = defineStore('backgroundAgent', {
  state: (): BackgroundAgentState => ({
    agents: [],
    selectedAgentId: null,
    isLoading: false,
    error: null,
    statusFilter: null,
  }),

  getters: {
    filteredAgents(): BackgroundAgent[] {
      if (!this.statusFilter) return this.agents
      return this.agents.filter((a) => a.status === this.statusFilter)
    },

    selectedAgent(): BackgroundAgent | null {
      if (!this.selectedAgentId) return null
      return this.agents.find((a) => a.id === this.selectedAgentId) ?? null
    },

    runningCount(): number {
      return this.agents.filter((a) => a.status === 'running').length
    },

    /** Look up a background agent by its bound chat session ID. */
    agentBySessionId() {
      return (sessionId: string): BackgroundAgent | null =>
        this.agents.find((a) => a.chat_session_id === sessionId) ?? null
    },
  },

  actions: {
    async fetchAgents(): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        this.agents = await api.listBackgroundAgents()
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch agents'
        console.error('Failed to fetch background agents:', err)
      } finally {
        this.isLoading = false
      }
    },

    selectAgent(id: string | null): void {
      this.selectedAgentId = id
    },

    setStatusFilter(status: BackgroundAgentStatus | null): void {
      this.statusFilter = status
    },

    async pauseAgent(id: string): Promise<void> {
      this.error = null
      try {
        const updated = await api.pauseBackgroundAgent(id)
        this.updateAgentLocally(updated)
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to pause agent'
        console.error('Failed to pause background agent:', err)
      }
    },

    async resumeAgent(id: string): Promise<void> {
      this.error = null
      try {
        const updated = await api.resumeBackgroundAgent(id)
        this.updateAgentLocally(updated)
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to resume agent'
        console.error('Failed to resume background agent:', err)
      }
    },

    async cancelAgent(taskId: string): Promise<void> {
      this.error = null
      try {
        await api.cancelBackgroundAgent(taskId)
        await this.fetchAgents()
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to cancel agent'
        console.error('Failed to cancel background agent:', err)
      }
    },

    async runAgentNow(id: string): Promise<api.StreamingBackgroundAgentResponse | null> {
      this.error = null
      try {
        const response = await api.runBackgroundAgentStreaming(id)
        await this.fetchAgents()
        return response
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to run agent'
        console.error('Failed to run background agent:', err)
        return null
      }
    },

    async deleteAgent(id: string): Promise<boolean> {
      this.error = null
      try {
        const success = await api.deleteBackgroundAgent(id)
        if (success) {
          this.agents = this.agents.filter((a) => a.id !== id)
          if (this.selectedAgentId === id) {
            this.selectedAgentId = null
          }
        }
        return success
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to delete agent'
        console.error('Failed to delete background agent:', err)
        return false
      }
    },

    updateAgentLocally(agent: BackgroundAgent): void {
      const idx = this.agents.findIndex((a) => a.id === agent.id)
      if (idx >= 0) {
        this.agents[idx] = agent
      } else {
        this.agents.push(agent)
      }
    },
  },
})
