import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { getAgent, updateAgent, deleteAgent } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import type { ApiKeyConfig } from '@/types/generated/ApiKeyConfig'
import type { AIModel } from '@/types/generated/AIModel'
import { ElMessage, ElMessageBox } from 'element-plus'
import { getDefaultTemperature } from '@/utils/AIModels'

export interface AgentFormData {
  name: string
  model: AIModel
  prompt: string | null
  temperature: number | null
  api_key_config: ApiKeyConfig | null
  tools: string[]
}

export function useAgentEditor(agentId: string) {
  const router = useRouter()

  const agent = ref<StoredAgent | null>(null)
  const isLoading = ref(true) // Start with loading state since we'll load immediately
  const isSaving = ref(false)
  const error = ref<string | null>(null)

  // Editable form data
  const formData = ref<AgentFormData>({
    name: '',
    model: 'claude-sonnet-4-5',
    prompt: null,
    temperature: 0.7,
    api_key_config: null,
    tools: [],
  })

  // Track if there are unsaved changes
  const hasChanges = computed(() => {
    if (!agent.value) return false
    return (
      formData.value.name !== agent.value.name ||
      formData.value.model !== agent.value.agent.model ||
      formData.value.prompt !== agent.value.agent.prompt ||
      formData.value.temperature !== agent.value.agent.temperature ||
      JSON.stringify(formData.value.api_key_config) !==
        JSON.stringify(agent.value.agent.api_key_config) ||
      JSON.stringify(formData.value.tools) !== JSON.stringify(agent.value.agent.tools || [])
    )
  })

  // Load agent data
  async function loadAgent() {
    isLoading.value = true
    error.value = null
    try {
      const data = await getAgent(agentId)
      agent.value = data
      // Initialize form data
      formData.value = {
        name: data.name,
        model: data.agent.model,
        prompt: data.agent.prompt,
        temperature:
          data.agent.temperature !== null && data.agent.temperature !== undefined
            ? data.agent.temperature
            : getDefaultTemperature(data.agent.model),
        api_key_config: data.agent.api_key_config,
        tools: data.agent.tools || [],
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to load agent'
      console.error('Failed to load agent:', err)
    } finally {
      isLoading.value = false
    }
  }

  // Save changes
  async function saveAgent(): Promise<boolean> {
    if (!agent.value || !hasChanges.value) return true

    isSaving.value = true
    try {
      const updates: { name?: string; agent?: AgentNode } = {}

      if (formData.value.name !== agent.value.name) {
        updates.name = formData.value.name
      }

      const agentData: AgentNode = {
        model: formData.value.model as AgentNode['model'],
        prompt: formData.value.prompt?.trim() || null,
        temperature: formData.value.temperature,
        api_key_config: formData.value.api_key_config,
        tools: formData.value.tools.length > 0 ? formData.value.tools : null,
      }
      updates.agent = agentData

      const updatedAgent = await updateAgent(agent.value.id, updates)

      // Update local agent data
      agent.value = updatedAgent
      formData.value = {
        name: updatedAgent.name,
        model: updatedAgent.agent.model,
        prompt: updatedAgent.agent.prompt,
        temperature:
          updatedAgent.agent.temperature !== null && updatedAgent.agent.temperature !== undefined
            ? updatedAgent.agent.temperature
            : getDefaultTemperature(updatedAgent.agent.model),
        api_key_config: updatedAgent.agent.api_key_config,
        tools: updatedAgent.agent.tools || [],
      }

      ElMessage.success('Agent saved successfully')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save agent'
      ElMessage.error(message)
      return false
    } finally {
      isSaving.value = false
    }
  }

  // Delete agent
  async function handleDelete(): Promise<boolean> {
    if (!agent.value) return false

    try {
      await ElMessageBox.confirm('Are you sure you want to delete this agent?', 'Delete Agent', {
        confirmButtonText: 'Delete',
        cancelButtonText: 'Cancel',
        type: 'warning',
      })

      await deleteAgent(agent.value.id)
      ElMessage.success('Agent deleted successfully')
      router.push('/agents')
      return true
    } catch (err) {
      if (err !== 'cancel') {
        const message = err instanceof Error ? err.message : 'Failed to delete agent'
        ElMessage.error(message)
      }
      return false
    }
  }

  // Reset form to original values
  function resetForm() {
    if (!agent.value) return
    formData.value = {
      name: agent.value.name,
      model: agent.value.agent.model,
      prompt: agent.value.agent.prompt,
      temperature:
        agent.value.agent.temperature !== null && agent.value.agent.temperature !== undefined
          ? agent.value.agent.temperature
          : getDefaultTemperature(agent.value.agent.model),
      api_key_config: agent.value.agent.api_key_config,
      tools: agent.value.agent.tools || [],
    }
  }

  // Navigate back with unsaved changes check
  async function goBack(): Promise<void> {
    if (hasChanges.value) {
      try {
        await ElMessageBox.confirm(
          'You have unsaved changes. Are you sure you want to leave?',
          'Unsaved Changes',
          {
            confirmButtonText: 'Leave',
            cancelButtonText: 'Stay',
            type: 'warning',
          },
        )
        router.push('/agents')
      } catch {
        // User cancelled, stay on page
      }
    } else {
      router.push('/agents')
    }
  }

  return {
    // State
    agent,
    formData,
    isLoading,
    isSaving,
    error,
    hasChanges,
    // Actions
    loadAgent,
    saveAgent,
    handleDelete,
    resetForm,
    goBack,
  }
}
