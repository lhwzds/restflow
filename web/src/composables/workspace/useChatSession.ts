/**
 * Chat Session Composable
 *
 * Provides reactive chat session management for the SkillWorkspace.
 * Handles session lifecycle, messaging, and UI state.
 */

import { ref, computed, watch, onMounted } from 'vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import type { ChatRole } from '@/types/generated/ChatRole'

export interface UseChatSessionOptions {
  /** Auto-load sessions on mount */
  autoLoad?: boolean
  /** Agent ID to filter sessions by */
  agentId?: string
  /** Skill ID to filter sessions by */
  skillId?: string
  /** Auto-select most recent session */
  autoSelectRecent?: boolean
}

/**
 * Composable for managing chat sessions in the workspace.
 *
 * @example
 * ```vue
 * <script setup>
 * const {
 *   sessions,
 *   currentSession,
 *   messages,
 *   inputMessage,
 *   isLoading,
 *   isSending,
 *   createSession,
 *   sendMessage,
 * } = useChatSession({ agentId: 'my-agent', autoLoad: true })
 * </script>
 * ```
 */
export function useChatSession(options: UseChatSessionOptions = {}) {
  const store = useChatSessionStore()

  // Local UI state
  const inputMessage = ref('')
  const isExpanded = ref(false)

  // Computed from store
  const sessions = computed(() => store.filteredSummaries)
  const currentSession = computed(() => store.currentSession)
  const messages = computed(() => store.currentMessages)
  const isLoading = computed(() => store.isLoading)
  const isLoadingSession = computed(() => store.isLoadingSession)
  const isSending = computed(() => store.isSending)
  const error = computed(() => store.error)

  // Local computed
  const hasSession = computed(() => store.hasActiveSession)
  const hasMessages = computed(() => messages.value.length > 0)
  const canSend = computed(() => 
    inputMessage.value.trim().length > 0 && 
    !isSending.value && 
    hasSession.value
  )

  // Actions
  async function loadSessions(): Promise<void> {
    if (options.agentId) {
      store.setAgentFilter(options.agentId)
    }
    if (options.skillId) {
      store.setSkillFilter(options.skillId)
    }
    await store.fetchSummaries()

    // Auto-select most recent session if enabled and sessions exist
    const firstSession = sessions.value[0]
    if (options.autoSelectRecent && firstSession && !store.currentSessionId) {
      await store.selectSession(firstSession.id)
    }
  }

  async function createSession(
    agentId: string,
    model: string,
    name?: string
  ): Promise<ChatSession | null> {
    const session = await store.createSession(agentId, model, name, options.skillId)
    if (session) {
      isExpanded.value = true
    }
    return session
  }

  async function selectSession(id: string | null): Promise<void> {
    await store.selectSession(id)
    if (id) {
      isExpanded.value = true
    }
  }

  async function deleteSession(id: string): Promise<boolean> {
    return store.deleteSession(id)
  }

  async function renameSession(id: string, name: string): Promise<ChatSession | null> {
    return store.renameSession(id, name)
  }

  async function sendMessage(): Promise<void> {
    if (!canSend.value) return

    const content = inputMessage.value.trim()
    inputMessage.value = ''

    await store.sendMessage(content)
  }

  function clearInput(): void {
    inputMessage.value = ''
  }

  function toggleExpanded(): void {
    isExpanded.value = !isExpanded.value
  }

  function clearError(): void {
    store.clearError()
  }

  // Lifecycle
  onMounted(() => {
    if (options.autoLoad) {
      loadSessions()
    }
  })

  // Watch for filter changes
  watch(
    () => options.agentId,
    (newAgentId) => {
      if (newAgentId) {
        store.setAgentFilter(newAgentId)
        loadSessions()
      }
    }
  )

  watch(
    () => options.skillId,
    (newSkillId) => {
      if (newSkillId) {
        store.setSkillFilter(newSkillId)
        loadSessions()
      }
    }
  )

  return {
    // State from store
    sessions,
    currentSession,
    messages,
    isLoading,
    isLoadingSession,
    isSending,
    error,

    // Local UI state
    inputMessage,
    isExpanded,

    // Computed
    hasSession,
    hasMessages,
    canSend,

    // Actions
    loadSessions,
    createSession,
    selectSession,
    deleteSession,
    renameSession,
    sendMessage,
    clearInput,
    toggleExpanded,
    clearError,
  }
}

/**
 * Helper to create a user message object
 */
export function createUserMessage(content: string): ChatMessage {
  return {
    role: 'User' as ChatRole,
    content,
    timestamp: BigInt(Date.now()),
    execution: null,
  }
}

/**
 * Helper to create an assistant message object
 */
export function createAssistantMessage(content: string): ChatMessage {
  return {
    role: 'Assistant' as ChatRole,
    content,
    timestamp: BigInt(Date.now()),
    execution: null,
  }
}

/**
 * Helper to format message timestamp
 */
export function formatMessageTime(timestamp: bigint): string {
  const date = new Date(Number(timestamp))
  const now = new Date()
  const isToday = date.toDateString() === now.toDateString()

  if (isToday) {
    return date.toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/**
 * Helper to format session updated time
 */
export function formatSessionTime(timestamp: bigint): string {
  const date = new Date(Number(timestamp))
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / 60000)
  const diffHours = Math.floor(diffMins / 60)
  const diffDays = Math.floor(diffHours / 24)

  if (diffMins < 1) return 'Just now'
  if (diffMins < 60) return `${diffMins}m ago`
  if (diffHours < 24) return `${diffHours}h ago`
  if (diffDays < 7) return `${diffDays}d ago`

  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
  })
}

/** Return type of useChatSession - inferred from implementation */
export type ChatSessionComposable = ReturnType<typeof useChatSession>
