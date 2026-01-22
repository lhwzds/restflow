import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { getAgent, updateAgent, deleteAgent } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import type { ApiKeyConfig } from '@/types/generated/ApiKeyConfig'
import type { AIModel } from '@/types/generated/AIModel'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { getDefaultTemperature } from '@/utils/AIModels'

export interface AgentFormData {
  name: string
  model: AIModel
  prompt: string | undefined
  temperature: number | undefined
  api_key_config: ApiKeyConfig | undefined
  tools: string[]
}

export function useAgentEditor(agentId: string) {
  const router = useRouter()
  const toast = useToast()
  const { confirm } = useConfirm()

  const agent = ref<StoredAgent | null>(null)
  const isLoading = ref(true) // Start with loading state since we'll load immediately
  const isSaving = ref(false)
  const error = ref<string | null>(null)

  // Editable form data
  const formData = ref<AgentFormData>({
    name: '',
    model: 'claude-sonnet-4-5',
    prompt: undefined,
    temperature: 0.7,
    api_key_config: undefined,
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
        temperature: data.agent.temperature ?? getDefaultTemperature(data.agent.model),
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
        prompt: formData.value.prompt?.trim() || undefined,
        temperature: formData.value.temperature,
        api_key_config: formData.value.api_key_config,
        tools: formData.value.tools.length > 0 ? formData.value.tools : undefined,
      }
      updates.agent = agentData

      const updatedAgent = await updateAgent(agent.value.id, updates)

      // Update local agent data
      agent.value = updatedAgent
      formData.value = {
        name: updatedAgent.name,
        model: updatedAgent.agent.model,
        prompt: updatedAgent.agent.prompt,
        temperature: updatedAgent.agent.temperature ?? getDefaultTemperature(updatedAgent.agent.model),
        api_key_config: updatedAgent.agent.api_key_config,
        tools: updatedAgent.agent.tools || [],
      }

      toast.success('Agent saved successfully')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save agent'
      toast.error(message)
      return false
    } finally {
      isSaving.value = false
    }
  }

  // Delete agent
  async function handleDelete(): Promise<boolean> {
    if (!agent.value) return false

    const confirmed = await confirm({
      title: 'Delete Agent',
      description: 'Are you sure you want to delete this agent?',
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'destructive',
    })

    if (!confirmed) return false

    try {
      await deleteAgent(agent.value.id)
      toast.success('Agent deleted successfully')
      router.push('/agents')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete agent'
      toast.error(message)
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
      temperature: agent.value.agent.temperature ?? getDefaultTemperature(agent.value.agent.model),
      api_key_config: agent.value.agent.api_key_config,
      tools: agent.value.agent.tools || [],
    }
  }

  // Navigate back with unsaved changes check
  async function goBack(): Promise<void> {
    if (hasChanges.value) {
      const confirmed = await confirm({
        title: 'Unsaved Changes',
        description: 'You have unsaved changes. Are you sure you want to leave?',
        confirmText: 'Leave',
        cancelText: 'Stay',
        variant: 'destructive',
      })
      if (confirmed) {
        router.push('/agents')
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
