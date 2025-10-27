<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import type { ApiKeyConfig } from '@/types/generated/ApiKeyConfig'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import { MODEL_OPTIONS } from '@/constants/node/models'
import ExpressionInput from '@/components/shared/ExpressionInput.vue'

interface AgentConfig {
  model?: string
  prompt?: string
  temperature?: number
  tools?: string[]
  input?: string
  api_key_config?: ApiKeyConfig | null
}

interface Props {
  modelValue: AgentConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: AgentConfig]
}>()

const availableTools = [
  { id: 'add', name: 'Addition Tool', description: 'Adds two numbers' },
  { id: 'get_current_time', name: 'Time Tool', description: 'Gets current time' },
]

const { buildConfig, extractConfig } = useApiKeyConfig()

const localData = ref<AgentConfig>({})

const keyMode = ref<'direct' | 'secret'>('direct')
const apiKeyDirect = ref('')
const apiKeySecret = ref('')

const { secrets, loadSecrets } = useSecretsData()

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = { ...newValue }
    if (!localData.value.tools) {
      localData.value.tools = []
    }
    if (newValue.api_key_config) {
      const { mode, value } = extractConfig(newValue.api_key_config)
      keyMode.value = mode
      if (mode === 'direct') {
        apiKeyDirect.value = value
      } else {
        apiKeySecret.value = value
      }
    }
  },
  { immediate: true },
)

onMounted(() => {
  loadSecrets()
})

const updateData = () => {
  const apiKeyValue = keyMode.value === 'direct' ? apiKeyDirect.value : apiKeySecret.value
  const apiKeyConfig = buildConfig(keyMode.value, apiKeyValue)

  emit('update:modelValue', {
    ...localData.value,
    api_key_config: apiKeyConfig
  })
}

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

const isToolSelected = (toolId: string) => {
  return localData.value.tools?.includes(toolId) || false
}
</script>

<template>
  <div class="agent-config">
    <div class="form-group">
      <label>Model</label>
      <select v-model="localData.model" @change="updateData">
        <option value="">Select a model</option>
        <optgroup
          v-for="provider in ['openai', 'anthropic', 'deepseek']"
          :key="provider"
          :label="provider.charAt(0).toUpperCase() + provider.slice(1)"
        >
          <option
            v-for="option in MODEL_OPTIONS.filter(opt => opt.provider === provider)"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </optgroup>
      </select>
    </div>

    <div class="form-group">
      <label>Prompt</label>
      <ExpressionInput
        :model-value="localData.prompt || ''"
        :multiline="true"
        placeholder="Analyze the user data: {{node.http1.data.body.user}} and provide insights based on {{trigger.payload.request}}"
        @update:model-value="(val) => { localData.prompt = val; updateData(); }"
        class="agent-prompt-editor"
      />
      <span class="form-hint">Use {{}} syntax to reference variables</span>
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
      <span class="form-hint">0 = deterministic, 1 = creative (default: 0.7)</span>
    </div>

    <div class="form-group">
      <label>Input</label>
      <ExpressionInput
        :model-value="localData.input || ''"
        :multiline="true"
        placeholder="{{trigger.payload}} or custom input"
        @update:model-value="(val) => { localData.input = val; updateData(); }"
      />
      <span class="form-hint">Input data for the agent</span>
    </div>

    <div class="form-group">
      <label>API Key Configuration</label>
      <div class="api-key-mode">
        <label class="radio-option">
          <input
            type="radio"
            v-model="keyMode"
            value="direct"
            @change="updateData"
          />
          <span>Direct Input</span>
        </label>
        <label class="radio-option">
          <input
            type="radio"
            v-model="keyMode"
            value="secret"
            @change="updateData"
          />
          <span>Use Secret</span>
        </label>
      </div>

      <input
        v-if="keyMode === 'direct'"
        type="password"
        v-model="apiKeyDirect"
        @input="updateData"
        placeholder="Enter API Key"
        class="api-key-input"
      />

      <select
        v-else
        v-model="apiKeySecret"
        @change="updateData"
        class="api-key-select"
      >
        <option value="">Select a secret</option>
        <option
          v-for="secret in secrets"
          :key="secret.key"
          :value="secret.key"
        >
          {{ secret.description || secret.key }}
        </option>
      </select>
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
  margin-bottom: var(--rf-spacing-sm);
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

.api-key-mode {
  display: flex;
  gap: var(--rf-spacing-lg);
  margin-bottom: var(--rf-spacing-md);
}

.radio-option {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-xs);
  cursor: pointer;
}

.radio-option input[type='radio'] {
  width: auto;
  margin: 0;
}

.radio-option span {
  font-size: var(--rf-font-size-sm);
  color: var(--rf-color-text-regular);
}

.api-key-input,
.api-key-select {
  margin-top: var(--rf-spacing-sm);
}

.form-hint {
  display: block;
  margin-top: var(--rf-spacing-xs);
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-placeholder);
}

.agent-prompt-editor {
  min-height: 120px;
}
</style>
