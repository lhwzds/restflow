<script setup lang="ts">
import { ref, watch } from 'vue'

interface AgentConfig {
  model?: string
  prompt?: string
  temperature?: number
  tools?: string[]
  input?: string
  api_key?: string
}

interface Props {
  modelValue: AgentConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: AgentConfig]
}>()

// Available tools
const availableTools = [
  { id: 'add', name: 'Addition Tool', description: 'Adds two numbers' },
  { id: 'get_current_time', name: 'Time Tool', description: 'Gets current time' },
]

// Local copy of data
const localData = ref<AgentConfig>({})

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = { ...newValue }
    if (!localData.value.tools) {
      localData.value.tools = []
    }
  },
  { immediate: true },
)

// Update data
const updateData = () => {
  emit('update:modelValue', { ...localData.value })
}

// Toggle tool selection
const toggleTool = (toolId: string) => {
  if (!localData.value.tools) {
    localData.value.tools = []
  }
  const index = localData.value.tools.indexOf(toolId)
  if (index === -1) {
    localData.value.tools.push(toolId)
  } else {
    localData.value.tools.splice(index, 1)
  }
  updateData()
}

// Check if tool is selected
const isToolSelected = (toolId: string) => {
  return localData.value.tools?.includes(toolId) || false
}
</script>

<template>
  <div class="agent-config">
    <div class="form-group">
      <label>Model</label>
      <select v-model="localData.model" @change="updateData">
        <option value="gpt-4.1">GPT-4.1</option>
      </select>
    </div>

    <div class="form-group">
      <label>Prompt</label>
      <textarea
        v-model="localData.prompt"
        @input="updateData"
        placeholder="Enter the agent prompt..."
        rows="4"
      />
    </div>

    <div class="form-group">
      <label>Temperature</label>
      <input
        type="number"
        v-model.number="localData.temperature"
        @input="updateData"
        min="0"
        max="1"
        step="0.1"
      />
    </div>

    <div class="form-group">
      <label>Input</label>
      <textarea
        v-model="localData.input"
        @input="updateData"
        placeholder="Input for the agent..."
        rows="2"
      />
    </div>

    <div class="form-group">
      <label>API Key</label>
      <input
        type="password"
        v-model="localData.api_key"
        @input="updateData"
        placeholder="sk-..."
        required
      />
    </div>

    <div class="form-group">
      <label>Tools</label>
      <div class="tools-list">
        <div
          v-for="tool in availableTools"
          :key="tool.id"
          class="tool-item"
          :class="{ selected: isToolSelected(tool.id) }"
          @click="toggleTool(tool.id)"
        >
          <input
            type="checkbox"
            :checked="isToolSelected(tool.id)"
            @click.stop="toggleTool(tool.id)"
          />
          <div class="tool-info">
            <div class="tool-name">{{ tool.name }}</div>
            <div class="tool-desc">{{ tool.description }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.form-group {
  margin-bottom: var(--rf-spacing-xl);
}

.form-group label {
  display: block;
  margin-bottom: var(--rf-spacing-xs-plus);
  font-size: var(--rf-font-size-base);
  font-weight: var(--rf-font-weight-medium);
  color: var(--rf-color-text-regular);
}

.form-group input,
.form-group select,
.form-group textarea {
  width: 100%;
  padding: var(--rf-spacing-sm) var(--rf-spacing-md);
  border: 1px solid var(--rf-color-border-light);
  border-radius: var(--rf-radius-base);
  font-size: var(--rf-font-size-base);
  transition: border-color var(--rf-transition-fast);
}

.form-group input:focus,
.form-group select:focus,
.form-group textarea:focus {
  outline: none;
  border-color: var(--rf-color-border-focus);
  box-shadow: var(--rf-shadow-focus);
}

.form-group textarea {
  resize: vertical;
  font-family: inherit;
}

.tools-list {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-sm);
}

.tool-item {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
  padding: var(--rf-spacing-md);
  border: 1px solid var(--rf-color-border-lighter);
  border-radius: var(--rf-radius-base);
  cursor: pointer;
  transition: all var(--rf-transition-fast);
}

.tool-item:hover {
  background-color: var(--rf-color-bg-secondary);
  border-color: var(--rf-color-border-light);
}

.tool-item.selected {
  background-color: var(--rf-color-primary-bg-lighter);
  border-color: var(--rf-color-border-focus);
}

.tool-item input[type='checkbox'] {
  width: auto;
  margin: 0;
}

.tool-info {
  flex: 1;
}

.tool-name {
  font-weight: var(--rf-font-weight-medium);
  font-size: var(--rf-font-size-base);
  color: var(--rf-color-text-primary);
}

.tool-desc {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  margin-top: var(--rf-spacing-3xs);
}
</style>
