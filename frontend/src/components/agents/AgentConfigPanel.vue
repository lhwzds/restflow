<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import {
  ElForm,
  ElFormItem,
  ElInput,
  ElSelect,
  ElOption,
  ElSlider,
  ElCheckboxGroup,
  ElCheckbox,
  ElButton,
  ElDivider,
  ElPopconfirm,
  ElMessage,
  ElRadioGroup,
  ElRadio
} from 'element-plus'
import { Check, Delete, CopyDocument } from '@element-plus/icons-vue'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'

const props = defineProps<{
  agent: StoredAgent
}>()

const emit = defineEmits<{
  update: [id: string, updates: { name?: string; agent?: AgentNode }]
  delete: [id: string]
}>()

const { secrets, loadSecrets: loadSecretsData } = useSecretsData()
const { buildConfig, isConfigChanged } = useApiKeyConfig()

const formData = ref({
  name: props.agent.name,
  model: props.agent.agent.model,
  prompt: props.agent.agent.prompt,
  temperature: props.agent.agent.temperature,
  api_key_config: props.agent.agent.api_key_config,
  tools: props.agent.agent.tools || []
})

// Computed properties for API key mode and value
const keyMode = computed({
  get: () => formData.value.api_key_config?.type || 'direct',
  set: (value: 'direct' | 'secret') => {
    formData.value.api_key_config = buildConfig(value, '')
  }
})

const apiKeyValue = computed({
  get: () => formData.value.api_key_config?.value || '',
  set: (value: string) => {
    const mode = keyMode.value as 'direct' | 'secret'
    formData.value.api_key_config = buildConfig(mode, value)
  }
})

watch(() => props.agent, (newAgent) => {
  formData.value = {
    name: newAgent.name,
    model: newAgent.agent.model,
    prompt: newAgent.agent.prompt,
    temperature: newAgent.agent.temperature,
    api_key_config: newAgent.agent.api_key_config,
    tools: newAgent.agent.tools || []
  }
}, { deep: true })

onMounted(async () => {
  await loadSecretsData()
})

const availableModels = [
  // OpenAI O Series (Reasoning models)
  { label: 'O4 Mini', value: 'o4-mini' },
  { label: 'O3', value: 'o3' },
  { label: 'O3 Mini', value: 'o3-mini' },
  { label: 'GPT-4.1', value: 'gpt-4.1' },
  { label: 'GPT-4.1 Mini', value: 'gpt-4.1-mini' },
  { label: 'GPT-4.1 Nano', value: 'gpt-4.1-nano' },
  { label: 'Claude 4 Opus', value: 'claude-4-opus' },
  { label: 'Claude 4 Sonnet', value: 'claude-4-sonnet' },
  { label: 'Claude 3.7 Sonnet', value: 'claude-3.7-sonnet' },
  { label: 'DeepSeek Chat', value: 'deepseek-chat' },
  { label: 'DeepSeek Reasoner', value: 'deepseek-reasoner' },
]

const availableTools = [
  { label: 'Addition Calculator', value: 'add' },
  { label: 'Get Current Time', value: 'get_current_time' }
]

const hasChanges = computed(() => {
  return (
    formData.value.name !== props.agent.name ||
    formData.value.model !== props.agent.agent.model ||
    formData.value.prompt !== props.agent.agent.prompt ||
    formData.value.temperature !== props.agent.agent.temperature ||
    isConfigChanged(props.agent.agent.api_key_config, formData.value.api_key_config) ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

function handleSave() {
  if (!formData.value.name.trim()) {
    ElMessage.error('Agent name cannot be empty')
    return
  }

  const updates: { name?: string; agent?: AgentNode } = {}

  if (formData.value.name !== props.agent.name) {
    updates.name = formData.value.name
  }

  const agentChanged =
    formData.value.model !== props.agent.agent.model ||
    formData.value.prompt !== props.agent.agent.prompt ||
    formData.value.temperature !== props.agent.agent.temperature ||
    isConfigChanged(props.agent.agent.api_key_config, formData.value.api_key_config) ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])

  if (agentChanged) {
    updates.agent = {
      model: formData.value.model,
      prompt: formData.value.prompt,
      temperature: formData.value.temperature,
      api_key_config: formData.value.api_key_config || null,
      tools: formData.value.tools.length > 0 ? formData.value.tools : null
    }
  }

  emit('update', props.agent.id, updates)
}

function handleDelete() {
  emit('delete', props.agent.id)
}

function handleCopyConfig() {
  const config = {
    model: formData.value.model,
    prompt: formData.value.prompt,
    temperature: formData.value.temperature,
    tools: formData.value.tools
  }

  navigator.clipboard.writeText(JSON.stringify(config, null, 2))
  ElMessage.success('Configuration copied to clipboard')
}

function resetForm() {
  formData.value = {
    name: props.agent.name,
    model: props.agent.agent.model,
    prompt: props.agent.agent.prompt,
    temperature: props.agent.agent.temperature,
    api_key_config: props.agent.agent.api_key_config,
    tools: props.agent.agent.tools || []
  }
}
</script>

<template>
  <div class="agent-config-panel">
    <ElForm :model="formData" label-position="top">
      <!-- Basic Information -->
      <div class="section">
        <h3 class="section-title">Basic Information</h3>
        <ElFormItem label="Agent Name" required>
          <ElInput
            v-model="formData.name"
            placeholder="Enter Agent name"
            clearable
          />
        </ElFormItem>
      </div>

      <ElDivider />

      <!-- Model Configuration -->
      <div class="section">
        <h3 class="section-title">Model Configuration</h3>
        <ElFormItem label="Select Model">
          <ElSelect v-model="formData.model" placeholder="Select model">
            <ElOption
              v-for="model in availableModels"
              :key="model.value"
              :label="model.label"
              :value="model.value"
            />
          </ElSelect>
        </ElFormItem>

        <ElFormItem label="Temperature">
          <div class="temperature-control">
            <ElSlider
              v-model="formData.temperature"
              :min="0"
              :max="2"
              :step="0.1"
              :show-tooltip="true"
            />
            <span class="temperature-value">{{ formData.temperature }}</span>
          </div>
        </ElFormItem>

        <ElFormItem label="API Key Configuration">
          <ElRadioGroup v-model="keyMode" style="margin-bottom: var(--rf-spacing-md)">
            <ElRadio value="direct">Direct Input</ElRadio>
            <ElRadio value="secret">Use Secret Manager</ElRadio>
          </ElRadioGroup>

          <ElInput
            v-if="keyMode === 'direct'"
            v-model="apiKeyValue"
            type="password"
            placeholder="Enter API Key (optional)"
            show-password
            clearable
          />

          <ElSelect
            v-else
            v-model="apiKeyValue"
            placeholder="Select a secret"
            clearable
            style="width: 100%"
          >
            <ElOption
              v-for="secret in secrets"
              :key="secret.key"
              :label="secret.description || secret.key"
              :value="secret.key"
            />
          </ElSelect>
        </ElFormItem>
      </div>

      <ElDivider />

      <!-- System Prompt -->
      <div class="section">
        <h3 class="section-title">System Prompt</h3>
        <ElFormItem>
          <ElInput
            v-model="formData.prompt"
            type="textarea"
            placeholder="Enter system prompt"
            :rows="8"
            :autosize="{ minRows: 6, maxRows: 20 }"
          />
        </ElFormItem>
      </div>

      <ElDivider />

      <!-- Tools Configuration -->
      <div class="section">
        <h3 class="section-title">Tools Configuration</h3>
        <ElFormItem>
          <ElCheckboxGroup v-model="formData.tools">
            <ElCheckbox
              v-for="tool in availableTools"
              :key="tool.value"
              :label="tool.value"
              :value="tool.value"
            >
              {{ tool.label }}
            </ElCheckbox>
          </ElCheckboxGroup>
        </ElFormItem>
      </div>

      <ElDivider />

      <!-- Action Buttons -->
      <div class="actions">
        <ElButton
          type="primary"
          :icon="Check"
          :disabled="!hasChanges"
          @click="handleSave"
        >
          Save Changes
        </ElButton>

        <ElButton
          :icon="CopyDocument"
          @click="handleCopyConfig"
        >
          Copy Config
        </ElButton>

        <ElButton
          v-if="hasChanges"
          @click="resetForm"
        >
          Reset
        </ElButton>

        <ElPopconfirm
          title="Are you sure you want to delete this Agent?"
          confirm-button-text="Confirm"
          cancel-button-text="Cancel"
          @confirm="handleDelete"
        >
          <template #reference>
            <ElButton
              type="danger"
              :icon="Delete"
            >
              Delete
            </ElButton>
          </template>
        </ElPopconfirm>
      </div>
    </ElForm>
  </div>
</template>

<style lang="scss" scoped>
.agent-config-panel {
  height: 100%;
  overflow-y: auto;
  padding: var(--rf-spacing-xl);
  background: var(--rf-color-bg-container);

  .section {
    margin-bottom: var(--rf-spacing-xl);

    .section-title {
      font-size: var(--rf-font-size-md);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
      margin-bottom: var(--rf-spacing-lg);
    }
  }

  .temperature-control {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xl);
    width: 100%;

    :deep(.el-slider) {
      flex: 1;
    }

    .temperature-value {
      min-width: var(--rf-size-sm);
      text-align: center;
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-primary);
      background: var(--rf-color-bg-secondary);
      padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
      border-radius: var(--rf-radius-small);
    }
  }

  .actions {
    display: flex;
    gap: var(--rf-spacing-lg);
    margin-top: var(--rf-spacing-2xl);
  }

  :deep(.el-divider--horizontal) {
    margin: var(--rf-spacing-xl) 0;
  }

  :deep(.el-checkbox-group) {
    display: flex;
    flex-direction: column;
    gap: var(--rf-spacing-lg);
  }

  :deep(.el-textarea__inner) {
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: var(--rf-font-size-sm);
    line-height: 1.5;
  }
}

// Dark mode adaptation
html.dark {
  .agent-config-panel {
    background-color: var(--rf-color-bg-container);

    .temperature-control .temperature-value {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>