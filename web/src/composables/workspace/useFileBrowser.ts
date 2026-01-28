import { ref, watch, type Ref } from 'vue'
import { listSkills } from '@/api/skills'
import { listAgents } from '@/api/agents'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { FileItem } from '@/types/workspace'

export type BrowserTab = 'agents' | 'skills'

export function useFileBrowser(activeTab: Ref<BrowserTab>) {
  const items = ref<FileItem<Skill | StoredAgent>[]>([])
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  // Transform Skill to FileItem
  function skillToFileItem(skill: Skill): FileItem<Skill> {
    return {
      id: skill.id,
      name: skill.name,
      path: `skills/${skill.id}`,
      isDirectory: false,
      updatedAt: skill.updated_at,
      data: skill,
    }
  }

  // Transform StoredAgent to FileItem
  function agentToFileItem(agent: StoredAgent): FileItem<StoredAgent> {
    return {
      id: agent.id,
      name: agent.name,
      path: `agents/${agent.id}`,
      isDirectory: false,
      updatedAt: agent.updated_at,
      data: agent,
    }
  }

  // Load items based on active tab
  async function loadItems() {
    isLoading.value = true
    error.value = null

    try {
      if (activeTab.value === 'skills') {
        const skills = await listSkills()
        items.value = skills.map(skillToFileItem)
      } else {
        const agents = await listAgents()
        items.value = agents.map(agentToFileItem)
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to load items'
      console.error('Failed to load items:', err)
      items.value = []
    } finally {
      isLoading.value = false
    }
  }

  // Reload when tab changes
  watch(activeTab, () => {
    loadItems()
  })

  return {
    items,
    isLoading,
    error,
    loadItems,
  }
}
