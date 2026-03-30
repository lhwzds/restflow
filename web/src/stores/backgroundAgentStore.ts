/**
 * Background Agent Store
 *
 * Pinia store for managing background agents in the workspace.
 * Provides listing, selection, control actions, and status filtering.
 */

import { defineStore } from 'pinia'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { BackgroundAgentStatus } from '@/types/generated/BackgroundAgentStatus'
import { BackendError } from '@/api/http-client'
import * as api from '@/api/background-agents'
import type { OperationAssessment } from '@/utils/operationAssessment'
import { extractOperationAssessment } from '@/utils/operationAssessment'

type AssessmentConfirmHandler = (assessment: OperationAssessment) => Promise<boolean>

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

    async stopAgent(taskId: string): Promise<void> {
      this.error = null
      try {
        await api.stopBackgroundAgent(taskId)
        await this.fetchAgents()
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to stop agent'
        console.error('Failed to stop background agent:', err)
      }
    },

    async runAgentNow(
      id: string,
      confirmWarning?: AssessmentConfirmHandler,
      confirmationToken?: string,
    ): Promise<api.StreamingBackgroundAgentResponse | null> {
      this.error = null
      try {
        const response = confirmationToken
          ? await api.runBackgroundAgentStreaming(id, confirmationToken)
          : await api.runBackgroundAgentStreaming(id)
        await this.fetchAgents()
        return response
      } catch (err) {
        const assessment = extractOperationAssessment(err)
        if (
          err instanceof BackendError &&
          err.code === 428 &&
          assessment?.confirmation_token &&
          confirmWarning
        ) {
          const confirmed = await confirmWarning(assessment)
          if (confirmed) {
            return this.runAgentNow(id, undefined, assessment.confirmation_token)
          }
          this.error = null
          return null
        }
        this.error = err instanceof Error ? err.message : 'Failed to run agent'
        console.error('Failed to run background agent:', err)
        return null
      }
    },

    async deleteAgent(id: string): Promise<boolean> {
      return this.deleteAgentWithConfirmation(id)
    },

    async deleteAgentWithConfirmation(
      id: string,
      confirmWarning?: AssessmentConfirmHandler,
      confirmationToken?: string,
    ): Promise<boolean> {
      this.error = null
      try {
        const success = confirmationToken
          ? await api.deleteBackgroundAgent(id, confirmationToken)
          : await api.deleteBackgroundAgent(id)
        if (success) {
          this.agents = this.agents.filter((a) => a.id !== id)
          if (this.selectedAgentId === id) {
            this.selectedAgentId = null
          }
        }
        return success
      } catch (err) {
        const assessment = extractOperationAssessment(err)
        if (
          err instanceof BackendError &&
          err.code === 428 &&
          assessment?.confirmation_token &&
          confirmWarning
        ) {
          const confirmed = await confirmWarning(assessment)
          if (confirmed) {
            return this.deleteAgentWithConfirmation(
              id,
              undefined,
              assessment.confirmation_token,
            )
          }
          this.error = null
          return false
        }
        this.error = err instanceof Error ? err.message : 'Failed to delete agent'
        console.error('Failed to delete background agent:', err)
        return false
      }
    },

    async convertSessionToAgent(
      request: api.ConvertSessionToBackgroundAgentRequest,
      confirmWarning?: AssessmentConfirmHandler,
    ): Promise<BackgroundAgent | null> {
      this.error = null
      try {
        const agent = await api.convertSessionToBackgroundAgent(request)
        this.agents.push(agent)
        return agent
      } catch (err) {
        const assessment = extractOperationAssessment(err)
        if (
          err instanceof BackendError &&
          err.code === 428 &&
          assessment?.confirmation_token &&
          confirmWarning
        ) {
          const confirmed = await confirmWarning(assessment)
          if (confirmed) {
            return this.convertSessionToAgent(
              {
                ...request,
                confirmation_token: assessment.confirmation_token,
              },
              undefined,
            )
          }
          this.error = null
          return null
        }
        this.error = err instanceof Error ? err.message : 'Failed to convert session'
        console.error('Failed to convert session to background agent:', err)
        return null
      }
    },

    /**
     * Convert a background-linked session back to a normal workspace session by
     * removing the background task binding while keeping the chat session.
     */
    async convertSessionToWorkspace(
      sessionId: string,
      confirmWarning?: AssessmentConfirmHandler,
    ): Promise<boolean> {
      this.error = null
      try {
        let target = this.agents.find((agent) => agent.chat_session_id === sessionId) ?? null
        if (!target) {
          await this.fetchAgents()
          target = this.agents.find((agent) => agent.chat_session_id === sessionId) ?? null
        }

        if (!target) {
          this.error = 'Background agent binding not found for this session'
          return false
        }

        const deleted = await this.deleteAgentWithConfirmation(target.id, confirmWarning)
        if (!deleted) {
          return false
        }
        return true
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to convert session'
        console.error('Failed to convert background session to workspace session:', err)
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
