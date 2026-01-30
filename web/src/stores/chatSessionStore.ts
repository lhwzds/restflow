/**
 * Chat Session Store
 *
 * Pinia store for managing chat sessions in the workspace.
 * Provides CRUD operations, filtering, and real-time state management.
 */

import { defineStore } from 'pinia'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import * as chatSessionApi from '@/api/chat-session'

export type SortField = 'name' | 'updated_at' | 'message_count'
export type SortOrder = 'asc' | 'desc'

interface ChatSessionState {
  /** Session summaries for efficient listing */
  summaries: ChatSessionSummary[]
  /** Full sessions (loaded on demand) */
  sessions: Map<string, ChatSession>
  /** Currently active session ID */
  currentSessionId: string | null
  /** Loading state for session list */
  isLoading: boolean
  /** Loading state for current session */
  isLoadingSession: boolean
  /** Sending message state */
  isSending: boolean
  /** Error message if any */
  error: string | null
  /** Filter by agent ID */
  agentFilter: string | null
  /** Filter by skill ID */
  skillFilter: string | null
  /** Current sort field */
  sortField: SortField
  /** Current sort order */
  sortOrder: SortOrder
  /** Search query for filtering sessions */
  searchQuery: string
  /** Version for reactive updates */
  version: number
}

export const useChatSessionStore = defineStore('chatSession', {
  state: (): ChatSessionState => ({
    summaries: [],
    sessions: new Map(),
    currentSessionId: null,
    isLoading: false,
    isLoadingSession: false,
    isSending: false,
    error: null,
    agentFilter: null,
    skillFilter: null,
    sortField: 'updated_at',
    sortOrder: 'desc',
    searchQuery: '',
    version: 0,
  }),

  getters: {
    /**
     * Get filtered and sorted session summaries
     */
    filteredSummaries(): ChatSessionSummary[] {
      let result = [...this.summaries]

      // Apply agent filter
      if (this.agentFilter) {
        result = result.filter((s) => s.agent_id === this.agentFilter)
      }

      // Apply search filter
      if (this.searchQuery.trim()) {
        const query = this.searchQuery.toLowerCase()
        result = result.filter(
          (s) =>
            s.name.toLowerCase().includes(query) ||
            (s.last_message_preview?.toLowerCase().includes(query) ?? false)
        )
      }

      // Apply sorting
      result.sort((a, b) => {
        let comparison = 0
        switch (this.sortField) {
          case 'name':
            comparison = a.name.localeCompare(b.name)
            break
          case 'updated_at':
            comparison = Number(a.updated_at - b.updated_at)
            break
          case 'message_count':
            comparison = a.message_count - b.message_count
            break
        }
        return this.sortOrder === 'asc' ? comparison : -comparison
      })

      return result
    },

    /**
     * Get the currently active session (full data)
     */
    currentSession(): ChatSession | null {
      if (!this.currentSessionId) return null
      return this.sessions.get(this.currentSessionId) ?? null
    },

    /**
     * Get the current session's messages
     */
    currentMessages(): ChatMessage[] {
      return this.currentSession?.messages ?? []
    },

    /**
     * Check if there are any sessions
     */
    hasSessions(): boolean {
      return this.summaries.length > 0
    },

    /**
     * Get session count
     */
    sessionCount(): number {
      return this.summaries.length
    },

    /**
     * Check if a session is currently active
     */
    hasActiveSession(): boolean {
      return this.currentSessionId !== null && this.currentSession !== null
    },
  },

  actions: {
    /**
     * Fetch session summaries from the API
     */
    async fetchSummaries(): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        this.summaries = await chatSessionApi.listChatSessionSummaries()
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch sessions'
        console.error('Failed to fetch chat session summaries:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Fetch sessions by agent
     */
    async fetchSessionsByAgent(agentId: string): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        const sessions = await chatSessionApi.listChatSessionsByAgent(agentId)
        // Update local cache
        sessions.forEach((s) => this.sessions.set(s.id, s))
        this.agentFilter = agentId
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch sessions'
        console.error('Failed to fetch chat sessions by agent:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Fetch sessions by skill
     */
    async fetchSessionsBySkill(skillId: string): Promise<void> {
      this.isLoading = true
      this.error = null
      try {
        const sessions = await chatSessionApi.listChatSessionsBySkill(skillId)
        // Update local cache
        sessions.forEach((s) => this.sessions.set(s.id, s))
        this.skillFilter = skillId
        this.version++
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to fetch sessions'
        console.error('Failed to fetch chat sessions by skill:', err)
      } finally {
        this.isLoading = false
      }
    },

    /**
     * Get a single session by ID (fetches if not cached)
     */
    async getSession(id: string): Promise<ChatSession | null> {
      // Return cached if available
      if (this.sessions.has(id)) {
        return this.sessions.get(id)!
      }

      this.isLoadingSession = true
      try {
        const session = await chatSessionApi.getChatSession(id)
        this.sessions.set(id, session)
        this.version++
        return session
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to get session'
        console.error('Failed to get chat session:', err)
        return null
      } finally {
        this.isLoadingSession = false
      }
    },

    /**
     * Create a new chat session
     */
    async createSession(
      agentId: string,
      model: string,
      name?: string,
      skillId?: string
    ): Promise<ChatSession | null> {
      this.error = null
      try {
        const session = await chatSessionApi.createChatSession({
          agentId,
          model,
          name,
          skillId,
        })
        // Add to cache
        this.sessions.set(session.id, session)
        // Add to summaries
        const summary: ChatSessionSummary = {
          id: session.id,
          name: session.name,
          agent_id: session.agent_id,
          model: session.model,
          skill_id: session.skill_id ?? null,
          message_count: 0,
          updated_at: session.updated_at,
          last_message_preview: null,
        }
        this.summaries.unshift(summary)
        this.currentSessionId = session.id
        this.version++
        return session
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to create session'
        console.error('Failed to create chat session:', err)
        return null
      }
    },

    /**
     * Delete a chat session
     */
    async deleteSession(id: string): Promise<boolean> {
      this.error = null
      try {
        const success = await chatSessionApi.deleteChatSession(id)
        if (success) {
          this.sessions.delete(id)
          this.summaries = this.summaries.filter((s) => s.id !== id)
          if (this.currentSessionId === id) {
            this.currentSessionId = null
          }
          this.version++
        }
        return success
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to delete session'
        console.error('Failed to delete chat session:', err)
        return false
      }
    },

    /**
     * Rename a chat session
     */
    async renameSession(id: string, name: string): Promise<ChatSession | null> {
      this.error = null
      try {
        const session = await chatSessionApi.renameChatSession(id, name)
        this.sessions.set(id, session)
        // Update summary
        const summary = this.summaries.find((s) => s.id === id)
        if (summary) {
          summary.name = name
        }
        this.version++
        return session
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to rename session'
        console.error('Failed to rename chat session:', err)
        return null
      }
    },

    /**
     * Send a message in the current session
     */
    async sendMessage(content: string): Promise<ChatSession | null> {
      if (!this.currentSessionId) {
        this.error = 'No active session'
        return null
      }

      this.isSending = true
      this.error = null
      try {
        const session = await chatSessionApi.sendChatMessage(this.currentSessionId, content)
        this.sessions.set(session.id, session)
        // Update summary
        const summary = this.summaries.find((s) => s.id === session.id)
        if (summary) {
          summary.message_count = session.messages.length
          summary.updated_at = session.updated_at
          summary.last_message_preview = content.slice(0, 100)
        }
        this.version++
        return session
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to send message'
        console.error('Failed to send chat message:', err)
        return null
      } finally {
        this.isSending = false
      }
    },

    /**
     * Add a message to a session (for local updates)
     */
    async addMessage(sessionId: string, message: ChatMessage): Promise<ChatSession | null> {
      this.error = null
      try {
        const session = await chatSessionApi.addChatMessage(sessionId, message)
        this.sessions.set(sessionId, session)
        // Update summary
        const summary = this.summaries.find((s) => s.id === sessionId)
        if (summary) {
          summary.message_count = session.messages.length
          summary.updated_at = session.updated_at
          if (message.content) {
            summary.last_message_preview = message.content.slice(0, 100)
          }
        }
        this.version++
        return session
      } catch (err) {
        this.error = err instanceof Error ? err.message : 'Failed to add message'
        console.error('Failed to add chat message:', err)
        return null
      }
    },

    /**
     * Select a session and load its full data
     */
    async selectSession(id: string | null): Promise<void> {
      this.currentSessionId = id
      if (id && !this.sessions.has(id)) {
        await this.getSession(id)
      }
    },

    /**
     * Update a session in local state (for real-time updates)
     */
    updateSessionLocally(session: ChatSession): void {
      this.sessions.set(session.id, session)
      // Update summary if exists
      const summary = this.summaries.find((s) => s.id === session.id)
      if (summary) {
        summary.message_count = session.messages.length
        summary.updated_at = session.updated_at
        const lastMessage = session.messages[session.messages.length - 1]
        if (lastMessage) {
          summary.last_message_preview = lastMessage.content.slice(0, 100)
        }
      }
      this.version++
    },

    /**
     * Set agent filter
     */
    setAgentFilter(agentId: string | null): void {
      this.agentFilter = agentId
    },

    /**
     * Set skill filter
     */
    setSkillFilter(skillId: string | null): void {
      this.skillFilter = skillId
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
      this.agentFilter = null
      this.skillFilter = null
      this.searchQuery = ''
      this.sortField = 'updated_at'
      this.sortOrder = 'desc'
    },

    /**
     * Clear error
     */
    clearError(): void {
      this.error = null
    },

    /**
     * Reset store state
     */
    reset(): void {
      this.summaries = []
      this.sessions.clear()
      this.currentSessionId = null
      this.isLoading = false
      this.isLoadingSession = false
      this.isSending = false
      this.error = null
      this.agentFilter = null
      this.skillFilter = null
      this.sortField = 'updated_at'
      this.sortOrder = 'desc'
      this.searchQuery = ''
      this.version = 0
    },
  },
})
