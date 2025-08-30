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
  margin-bottom: 20px;
}

.form-group label {
  display: block;
  margin-bottom: 6px;
  font-size: 14px;
  font-weight: 500;
  color: #475569;
}

.form-group input,
.form-group select,
.form-group textarea {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  font-size: 14px;
  transition: border-color 0.2s;
}

.form-group input:focus,
.form-group select:focus,
.form-group textarea:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.1);
}

.form-group textarea {
  resize: vertical;
  font-family: inherit;
}

.tools-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border: 1px solid #e2e8f0;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.tool-item:hover {
  background-color: #f8fafc;
  border-color: #cbd5e1;
}

.tool-item.selected {
  background-color: #eff6ff;
  border-color: #6366f1;
}

.tool-item input[type='checkbox'] {
  width: auto;
  margin: 0;
}

.tool-info {
  flex: 1;
}

.tool-name {
  font-weight: 500;
  font-size: 14px;
  color: #1e293b;
}

.tool-desc {
  font-size: 12px;
  color: #64748b;
  margin-top: 2px;
}
</style>
