/**
 * Terminal Sessions Management
 *
 * Manages persistent terminal sessions stored in redb via Tauri IPC.
 * Sessions persist across page reloads, allowing terminals to be
 * reopened like files.
 */

import { ref, readonly, computed } from 'vue'
import type { TerminalSession } from '@/types/generated/TerminalSession'
import * as terminalApi from '@/api/terminal-sessions'
import { restartTerminal } from '@/api/pty'

// Re-export the type with camelCase field name mapping for frontend convenience
export type { TerminalSession }

// Global singleton state
const sessions = ref<TerminalSession[]>([])
const isLoading = ref(false)
const isInitialized = ref(false)

/**
 * Load sessions from the backend
 */
async function loadSessions() {
  if (isLoading.value) return

  isLoading.value = true
  try {
    sessions.value = await terminalApi.listTerminalSessions()
    isInitialized.value = true
  } catch (error) {
    console.error('Failed to load terminal sessions:', error)
    sessions.value = []
  } finally {
    isLoading.value = false
  }
}

// Initialize on module load (async)
loadSessions()

export function useTerminalSessions() {
  /**
   * Ensure sessions are loaded (for components that need fresh data)
   */
  async function ensureLoaded() {
    if (!isInitialized.value) {
      await loadSessions()
    }
  }

  /**
   * Refresh sessions from the backend
   */
  async function refreshSessions() {
    await loadSessions()
  }

  /**
   * Create a new terminal session
   */
  async function createSession(): Promise<TerminalSession> {
    try {
      const session = await terminalApi.createTerminalSession()
      // Add to local state
      sessions.value.push(session)
      return session
    } catch (error) {
      console.error('Failed to create terminal session:', error)
      throw error
    }
  }

  /**
   * Delete a terminal session
   */
  async function deleteSession(id: string): Promise<void> {
    try {
      await terminalApi.deleteTerminalSession(id)
      // Remove from local state
      const index = sessions.value.findIndex((s) => s.id === id)
      if (index !== -1) {
        sessions.value.splice(index, 1)
      }
    } catch (error) {
      console.error('Failed to delete terminal session:', error)
      throw error
    }
  }

  /**
   * Rename a terminal session
   */
  async function renameSession(id: string, name: string): Promise<TerminalSession> {
    try {
      const updated = await terminalApi.renameTerminalSession(id, name)
      // Update local state
      const index = sessions.value.findIndex((s) => s.id === id)
      if (index !== -1) {
        sessions.value[index] = updated
      }
      return updated
    } catch (error) {
      console.error('Failed to rename terminal session:', error)
      throw error
    }
  }

  /**
   * Get a session by id (from local state)
   */
  function getSession(id: string): TerminalSession | undefined {
    return sessions.value.find((s) => s.id === id)
  }

  /**
   * Restart a stopped terminal session
   */
  async function restartSession(id: string): Promise<TerminalSession> {
    try {
      const updated = await restartTerminal(id)
      // Update local state
      const index = sessions.value.findIndex((s) => s.id === id)
      if (index !== -1) {
        sessions.value[index] = updated
      }
      return updated
    } catch (error) {
      console.error('Failed to restart terminal session:', error)
      throw error
    }
  }

  /**
   * Update a session in local state (for external updates)
   */
  function updateSessionLocal(session: TerminalSession): void {
    const index = sessions.value.findIndex((s) => s.id === session.id)
    if (index !== -1) {
      sessions.value[index] = session
    }
  }

  /**
   * Update a terminal session's configuration
   */
  async function updateSession(
    id: string,
    params: {
      name?: string
      working_directory?: string | null
      startup_command?: string | null
    },
  ): Promise<TerminalSession> {
    try {
      const updated = await terminalApi.updateTerminalSession(id, params)
      // Update local state
      const index = sessions.value.findIndex((s) => s.id === id)
      if (index !== -1) {
        sessions.value[index] = updated
      }
      return updated
    } catch (error) {
      console.error('Failed to update terminal session:', error)
      throw error
    }
  }

  // Computed filters
  const runningSessions = computed(() => sessions.value.filter((s) => s.status === 'running'))

  const stoppedSessions = computed(() => sessions.value.filter((s) => s.status === 'stopped'))

  return {
    sessions: readonly(sessions),
    runningSessions,
    stoppedSessions,
    isLoading: readonly(isLoading),
    isInitialized: readonly(isInitialized),
    ensureLoaded,
    refreshSessions,
    createSession,
    deleteSession,
    renameSession,
    updateSession,
    getSession,
    restartSession,
    updateSessionLocal,
  }
}
