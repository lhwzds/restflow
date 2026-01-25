<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { X, Save, Loader2, Bot, Settings, Plus } from 'lucide-vue-next'
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
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
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
const name = ref('')
const model = ref<AIModel>('claude-sonnet-4-5')
const prompt = ref('')
const temperature = ref(0.7)
const tools = ref<string[]>([])

// Tool management
const {
  isLoading: isLoadingTools,
  selectedToolValue,
  loadTools,
  addTool,
  removeTool,
  getToolLabel,
  getAvailableTools,
} = useAgentTools(tools)

// Get all available models
const models = computed(() => getAllModels())

// Check if current model supports temperature
const showTemperature = computed(() => supportsTemperature(model.value))

// Initialize form data from agent
watch(
  () => props.agent,
  (agent) => {
    if (agent) {
      name.value = agent.name
      model.value = agent.agent.model
      prompt.value = agent.agent.prompt || ''
      temperature.value = agent.agent.temperature ?? getDefaultTemperature(agent.agent.model) ?? 0.7
      tools.value = [...(agent.agent.tools || [])]
    } else {
      name.value = ''
      model.value = 'claude-sonnet-4-5'
      prompt.value = ''
      temperature.value = 0.7
      tools.value = []
    }
  },
  { immediate: true },
)

// Handle model change
function onModelChange(value: string) {
  const newModel = value as AIModel
  model.value = newModel
  // Reset temperature when switching to a model that doesn't support it
  if (!supportsTemperature(newModel)) {
    temperature.value = 0.7
  }
}

// Handle temperature slider change
function onTemperatureChange(value: number[] | undefined) {
  if (value && value[0] !== undefined) {
    temperature.value = value[0]
  }
}

// Check if form has changes
const hasChanges = computed(() => {
  if (!props.agent) return name.value.trim() !== ''
  return (
    name.value !== props.agent.name ||
    model.value !== props.agent.agent.model ||
    prompt.value !== (props.agent.agent.prompt || '') ||
    temperature.value !== (props.agent.agent.temperature ?? 0.7) ||
    JSON.stringify(tools.value) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

// Check if can save
const canSave = computed(() => {
  if (!props.agent) {
    return name.value.trim() !== '' && name.value !== 'Untitled'
  }
  return hasChanges.value
})

// Save the agent
async function handleSave() {
  if (!name.value.trim()) {
    toast.error('Name is required')
    return
  }

  isSaving.value = true
  try {
    const agentNode: AgentNode = {
      model: model.value,
      prompt: prompt.value.trim() || undefined,
      temperature: showTemperature.value ? temperature.value : undefined,
      tools: tools.value.length > 0 ? tools.value : undefined,
    }

    let savedAgent: StoredAgent

    if (props.isNew || !props.agent) {
      // Create new agent
      savedAgent = await createAgent({
        name: name.value.trim(),
        agent: agentNode,
      })
      toast.success('Agent created successfully')
    } else {
      // Update existing agent
      savedAgent = await updateAgent(props.agent.id, {
        name: name.value.trim(),
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
    <!-- Header: Icon + Name + Actions (similar to SkillEditor) -->
    <div v-if="showHeader" class="h-11 border-b flex items-center px-3 gap-3 shrink-0">
      <Bot :size="18" class="text-muted-foreground shrink-0" />

      <!-- Editable name -->
      <Input
        v-model="name"
        class="h-7 text-sm font-medium border-none shadow-none focus-visible:ring-0 px-1 bg-transparent"
        :class="{ 'text-muted-foreground italic': name === 'Untitled' || name === '' }"
        placeholder="Enter agent name..."
      />

      <span class="text-muted-foreground text-sm">.agent</span>

      <div class="flex-1" />

      <!-- Actions -->
      <Button variant="ghost" size="sm" class="h-7" :disabled="isSaving" @click="emit('cancel')">
        <X :size="14" class="mr-1" />
        Cancel
      </Button>
      <Button size="sm" class="h-7" :disabled="isSaving || !canSave" @click="handleSave">
        <Loader2 v-if="isSaving" :size="14" class="mr-1 animate-spin" />
        <Save v-else :size="14" class="mr-1" />
        Save
      </Button>
    </div>

    <!-- Main Editor with Floating Config -->
    <div class="flex-1 relative">
      <!-- Floating Config Button (top-right) -->
      <Popover>
        <PopoverTrigger as-child>
          <Button
            variant="outline"
            size="icon"
            class="absolute top-3 right-6 z-10 h-8 w-8 bg-background/80 backdrop-blur-sm"
          >
            <Settings :size="16" />
          </Button>
        </PopoverTrigger>
        <PopoverContent class="w-80" align="end">
          <div class="space-y-4">
            <!-- Model Select -->
            <div class="space-y-2">
              <Label>Model</Label>
              <Select :model-value="model" @update:model-value="onModelChange">
                <SelectTrigger class="w-full">
                  <SelectValue placeholder="Select a model" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="m in models" :key="m.value" :value="m.value">
                    {{ m.label }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            <!-- Temperature Slider -->
            <div v-if="showTemperature" class="space-y-2">
              <Label>Temperature: {{ temperature.toFixed(1) }}</Label>
              <Slider
                :model-value="[temperature]"
                :min="0"
                :max="2"
                :step="0.1"
                class="w-full"
                @update:model-value="onTemperatureChange"
              />
              <p class="text-xs text-muted-foreground">
                Lower = focused, Higher = creative
              </p>
            </div>

            <!-- Tools -->
            <div class="space-y-2">
              <Label>Tools</Label>
              <div class="flex flex-wrap gap-1.5 mb-2">
                <Badge
                  v-for="tool in tools"
                  :key="tool"
                  variant="secondary"
                  class="text-xs gap-1 pr-1"
                >
                  {{ getToolLabel(tool) }}
                  <button type="button" class="hover:text-destructive" @click="removeTool(tool)">
                    <X :size="12" />
                  </button>
                </Badge>
                <span v-if="tools.length === 0" class="text-xs text-muted-foreground">
                  No tools selected
                </span>
              </div>
              <div class="flex gap-2">
                <Select v-model="selectedToolValue">
                  <SelectTrigger class="flex-1 h-8">
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
                  class="h-8 w-8"
                  :disabled="!selectedToolValue || isLoadingTools"
                  @click="addTool"
                >
                  <Plus :size="14" />
                </Button>
              </div>
            </div>
          </div>
        </PopoverContent>
      </Popover>

      <!-- Main Textarea (full height) -->
      <Textarea
        v-model="prompt"
        class="h-full resize-none border-0 rounded-none focus-visible:ring-0 font-mono text-sm p-4 bg-background"
        placeholder="Write your system prompt here..."
      />
    </div>
  </div>
</template>
