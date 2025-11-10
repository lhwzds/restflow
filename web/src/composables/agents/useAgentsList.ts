import { ref, computed } from 'vue'
import { listAgents } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const agents = ref<StoredAgent[]>([])
const isLoading = ref(false)
const searchQuery = ref('')
const selectedAgent = ref<StoredAgent | null>(null)

const filteredAgents = computed(() => {
  if (!searchQuery.value) return agents.value

  const query = searchQuery.value.toLowerCase()
  return agents.value.filter(
    (agent) =>
      agent.name.toLowerCase().includes(query) ||
      agent.agent.model.toLowerCase().includes(query) ||
      (agent.agent.prompt || '').toLowerCase().includes(query),
  )
})

async function loadAgents() {
  isLoading.value = true
  try {
    agents.value = await listAgents()
  } catch (error: any) {
    console.error('Failed to load agents:', error)
    throw error
  } finally {
    isLoading.value = false
  }
}

function selectAgent(agent: StoredAgent | null) {
  selectedAgent.value = agent
}

function clearSelection() {
  selectedAgent.value = null
}

export function useAgentsList() {
  return {
    agents,
    isLoading,
    searchQuery,
    selectedAgent,
    filteredAgents,
    loadAgents,
    selectAgent,
    clearSelection,
  }
}
