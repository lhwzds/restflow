<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { X, Save, Loader2, Plus } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { createAgent, updateAgent } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import type { AIModel } from '@/types/generated/AIModel'
import { useToast } from '@/composables/useToast'
import { useAgentTools } from '@/composables/agents/useAgentTools'
import { getAllModels, getDefaultTemperature, supportsTemperature } from '@/utils/AIModels'
import { useModelsStore } from '@/stores/modelsStore'

const props = withDefaults(
  defineProps<{
    agent?: StoredAgent | null
    isNew?: boolean
    showHeader?: boolean
  }>(),
  {
    showHeader: true,
  },
)

const emit = defineEmits<{
  save: [agent: StoredAgent]
  cancel: []
}>()

const toast = useToast()
const modelsStore = useModelsStore()
const isSaving = ref(false)

// Form data
const formData = ref({
  name: '',
  model: 'claude-sonnet-4-5' as AIModel,
  prompt: '',
  temperature: 0.7,
  tools: [] as string[],
})

// Tool management - create a ref that syncs with formData.tools
const formableTools = ref<string[]>([])

const {
  isLoading: isLoadingTools,
  selectedToolValue,
  loadTools,
  addTool,
  removeTool,
  getToolLabel,
  getAvailableTools,
} = useAgentTools(formableTools)

// Sync formableTools with formData.tools
watch(
  () => formData.value.tools,
  (newTools) => {
    formableTools.value = [...newTools]
  },
  { deep: true },
)

watch(
  formableTools,
  (newTools) => {
    formData.value.tools = [...newTools]
  },
  { deep: true },
)

// Get all available models
const models = computed(() => getAllModels())

// Check if current model supports temperature
const showTemperature = computed(() => supportsTemperature(formData.value.model))

// Initialize form data from agent
watch(
  () => props.agent,
  (agent) => {
    if (agent) {
      formData.value = {
        name: agent.name,
        model: agent.agent.model,
        prompt: agent.agent.prompt || '',
        temperature: agent.agent.temperature ?? getDefaultTemperature(agent.agent.model) ?? 0.7,
        tools: [...(agent.agent.tools || [])],
      }
    } else {
      formData.value = {
        name: '',
        model: 'claude-sonnet-4-5',
        prompt: '',
        temperature: 0.7,
        tools: [],
      }
    }
  },
  { immediate: true },
)

// Handle model change
function onModelChange(value: string) {
  const model = value as AIModel
  formData.value.model = model
  // Reset temperature when switching to a model that doesn't support it
  if (!supportsTemperature(model)) {
    formData.value.temperature = 0.7
  }
}

// Handle temperature slider change
function onTemperatureChange(value: number[] | undefined) {
  if (value && value[0] !== undefined) {
    formData.value.temperature = value[0]
  }
}

// Check if form has changes
const hasChanges = computed(() => {
  if (!props.agent) return formData.value.name.trim() !== ''
  return (
    formData.value.name !== props.agent.name ||
    formData.value.model !== props.agent.agent.model ||
    formData.value.prompt !== (props.agent.agent.prompt || '') ||
    formData.value.temperature !== (props.agent.agent.temperature ?? 0.7) ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

// Save the agent
async function handleSave() {
  if (!formData.value.name.trim()) {
    toast.error('Name is required')
    return
  }

  isSaving.value = true
  try {
    const agentNode: AgentNode = {
      model: formData.value.model,
      prompt: formData.value.prompt.trim() || undefined,
      temperature: showTemperature.value ? formData.value.temperature : undefined,
      tools: formData.value.tools.length > 0 ? formData.value.tools : undefined,
    }

    let savedAgent: StoredAgent

    if (props.isNew || !props.agent) {
      // Create new agent
      savedAgent = await createAgent({
        name: formData.value.name.trim(),
        agent: agentNode,
      })
      toast.success('Agent created successfully')
    } else {
      // Update existing agent
      savedAgent = await updateAgent(props.agent.id, {
        name: formData.value.name.trim(),
        agent: agentNode,
      })
      toast.success('Agent saved successfully')
    }

    emit('save', savedAgent)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to save agent'
    toast.error(message)
  } finally {
    isSaving.value = false
  }
}

// Load models and tools on mount
onMounted(async () => {
  await modelsStore.loadModels()
  await loadTools()
})
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Header (conditional) -->
    <div v-if="showHeader" class="h-12 border-b flex items-center px-4 justify-between shrink-0">
      <h2 class="text-lg font-semibold">
        {{ isNew ? 'New Agent' : 'Edit Agent' }}
      </h2>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" :disabled="isSaving" @click="emit('cancel')">
          <X :size="16" class="mr-1" />
          Cancel
        </Button>
        <Button size="sm" :disabled="isSaving || !hasChanges" @click="handleSave">
          <Loader2 v-if="isSaving" :size="16" class="mr-1 animate-spin" />
          <Save v-else :size="16" class="mr-1" />
          Save
        </Button>
      </div>
    </div>

    <!-- Form -->
    <div class="flex-1 overflow-auto p-4">
      <div class="max-w-[48rem] mx-auto space-y-6">
        <!-- Name -->
        <div class="space-y-2">
          <Label for="name">Name</Label>
          <Input id="name" v-model="formData.name" placeholder="Enter agent name" />
        </div>

        <!-- Model -->
        <div class="space-y-2">
          <Label for="model">Model</Label>
          <Select :model-value="formData.model" @update:model-value="onModelChange">
            <SelectTrigger class="w-full">
              <SelectValue placeholder="Select a model" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="model in models" :key="model.value" :value="model.value">
                {{ model.label }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- Temperature -->
        <div v-if="showTemperature" class="space-y-2">
          <Label>Temperature: {{ formData.temperature.toFixed(1) }}</Label>
          <Slider
            :model-value="[formData.temperature]"
            :min="0"
            :max="2"
            :step="0.1"
            class="w-full"
            @update:model-value="onTemperatureChange"
          />
          <p class="text-xs text-muted-foreground">
            Lower values produce more focused output, higher values produce more creative output.
          </p>
        </div>

        <!-- Tools -->
        <div class="space-y-2">
          <Label>Tools</Label>
          <div class="flex flex-wrap gap-2 mb-2">
            <Badge
              v-for="tool in formData.tools"
              :key="tool"
              variant="secondary"
              class="text-xs gap-1 pr-1"
            >
              {{ getToolLabel(tool) }}
              <button type="button" class="hover:text-destructive" @click="removeTool(tool)">
                <X :size="12" />
              </button>
            </Badge>
            <span v-if="formData.tools.length === 0" class="text-xs text-muted-foreground">
              No tools selected
            </span>
          </div>
          <div class="flex gap-2">
            <Select v-model="selectedToolValue">
              <SelectTrigger class="flex-1">
                <SelectValue placeholder="Add a tool..." />
              </SelectTrigger>
              <SelectContent>
                <SelectItem
                  v-for="tool in getAvailableTools()"
                  :key="tool.value"
                  :value="tool.value"
                >
                  {{ tool.label }}
                </SelectItem>
              </SelectContent>
            </Select>
            <Button
              type="button"
              variant="outline"
              size="icon"
              :disabled="!selectedToolValue || isLoadingTools"
              @click="addTool"
            >
              <Plus :size="16" />
            </Button>
          </div>
        </div>

        <!-- System Prompt -->
        <div class="space-y-2">
          <Label for="prompt">System Prompt</Label>
          <Textarea
            id="prompt"
            v-model="formData.prompt"
            placeholder="Write the system prompt for this agent..."
            class="min-h-[200px] font-mono text-sm"
          />
        </div>
      </div>
    </div>
  </div>
</template>
