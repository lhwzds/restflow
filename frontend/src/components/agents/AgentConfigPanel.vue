<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import {
  ElForm,
  ElFormItem,
  ElInput,
  ElSelect,
  ElOption,
  ElSlider,
  ElDivider,
  ElRadioGroup,
  ElRadio,
  ElTag
} from 'element-plus'
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
  'changes-update': [hasChanges: boolean]
}>()

const { secrets, loadSecrets: loadSecretsData } = useSecretsData()
const { buildConfig, isConfigChanged } = useApiKeyConfig()

const formData = ref({
  name: props.agent.name,
  model: props.agent.agent.model,
  prompt: props.agent.agent.prompt,
  temperature: props.agent.agent.temperature ?? 0.7, // Default to 0.7 if null
  api_key_config: props.agent.agent.api_key_config,
  tools: props.agent.agent.tools || []
})

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
    temperature: newAgent.agent.temperature ?? 0.7, // Default to 0.7 if null
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
  { label: 'Addition Calculator', value: 'add', description: 'Adds two numbers together' },
  { label: 'Get Current Time', value: 'get_current_time', description: 'Returns the current system time' }
]

const selectedToolValue = ref('')

function addTool() {
  if (selectedToolValue.value && !formData.value.tools.includes(selectedToolValue.value)) {
    formData.value.tools.push(selectedToolValue.value)
    selectedToolValue.value = ''
  }
}

function removeTool(toolValue: string) {
  const index = formData.value.tools.indexOf(toolValue)
  if (index > -1) {
    formData.value.tools.splice(index, 1)
  }
}

function getToolLabel(value: string): string {
  const tool = availableTools.find(t => t.value === value)
  return tool?.label || value
}

const isOSeriesModel = computed(() => {
  return ['o4-mini', 'o3', 'o3-mini'].includes(formData.value.model)
})

const hasChanges = computed(() => {
  return (
    formData.value.name !== props.agent.name ||
    formData.value.model !== props.agent.agent.model ||
    (formData.value.prompt || null) !== (props.agent.agent.prompt || null) ||
    formData.value.temperature !== props.agent.agent.temperature ||
    isConfigChanged(props.agent.agent.api_key_config, formData.value.api_key_config) ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

watch(hasChanges,
 (newVal) => {
  emit('changes-update', newVal)
})

function saveChanges() {
  if (!formData.value.name.trim()) {
    return
  }

  const updates: { name?: string; agent?: AgentNode } = {}

  if (formData.value.name !== props.agent.name) {
    updates.name = formData.value.name
  }

  const agentChanged =
    formData.value.model !== props.agent.agent.model ||
    (formData.value.prompt || null) !== (props.agent.agent.prompt || null) ||
    formData.value.temperature !== props.agent.agent.temperature ||
    isConfigChanged(props.agent.agent.api_key_config, formData.value.api_key_config) ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])

  if (agentChanged) {
    updates.agent = {
      model: formData.value.model,
      prompt: formData.value.prompt?.trim() || null,
      temperature: isOSeriesModel.value ? null : formData.value.temperature,
      api_key_config: formData.value.api_key_config || null,
      tools: formData.value.tools.length > 0 ? formData.value.tools : null
    } as AgentNode
  }

  if (updates.name || updates.agent) {
    emit('update', props.agent.id, updates)
  }
}

function resetForm() {
  formData.value = {
    name: props.agent.name,
    model: props.agent.agent.model,
    prompt: props.agent.agent.prompt,
    temperature: props.agent.agent.temperature ?? 0.7, // Default to 0.7 if null
    api_key_config: props.agent.agent.api_key_config,
    tools: props.agent.agent.tools || []
  }
}

defineExpose({
  saveChanges,
  resetForm
})
</script>

<template>
  <div class="agent-config-panel">
    <ElForm :model="formData" label-position="top">
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

      <div class="section">
        <h3 class="section-title">Model Configuration</h3>
        <div class="model-row">
          <ElFormItem label="Model" :class="{ 'model-select': !isOSeriesModel, 'model-select-full': isOSeriesModel }">
            <ElSelect v-model="formData.model" placeholder="Select model">
              <ElOption
                v-for="model in availableModels"
                :key="model.value"
                :label="model.label"
                :value="model.value"
              />
            </ElSelect>
          </ElFormItem>

          <ElFormItem v-if="!isOSeriesModel" label="Temp" class="temperature-item">
            <div class="temperature-control compact">
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
        </div>

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

      <div class="section">
        <h3 class="section-title">System Prompt (Optional)</h3>
        <ElFormItem>
          <ElInput
            v-model="formData.prompt"
            type="textarea"
            placeholder="Enter system prompt (optional)"
            :rows="3"
            :autosize="{ minRows: 3, maxRows: 8 }"
          />
        </ElFormItem>
      </div>

      <ElDivider />

      <div class="section">
        <h3 class="section-title">Tools Configuration</h3>
        <ElFormItem>
          <div class="tools-selector">
            <ElSelect
              v-model="selectedToolValue"
              placeholder="Select a tool to add"
              clearable
              @change="addTool"
              style="width: 100%; margin-bottom: var(--rf-spacing-md)"
            >
              <ElOption
                v-for="tool in availableTools.filter(t => !formData.tools.includes(t.value))"
                :key="tool.value"
                :label="tool.label"
                :value="tool.value"
              >
                <div class="tool-option">
                  <div class="tool-label">{{ tool.label }}</div>
                  <div class="tool-description">{{ tool.description }}</div>
                </div>
              </ElOption>
            </ElSelect>

            <div v-if="formData.tools.length > 0" class="tools-tags">
              <ElTag
                v-for="toolValue in formData.tools"
                :key="toolValue"
                closable
                size="large"
                @close="removeTool(toolValue)"
              >
                {{ getToolLabel(toolValue) }}
              </ElTag>
            </div>
            <div v-else class="no-tools-hint">
              No tools selected
            </div>
          </div>
        </ElFormItem>
      </div>
    </ElForm>
  </div>
</template>

<style lang="scss" scoped>
.agent-config-panel {
  height: 100%;
  overflow-y: auto;
  padding: var(--rf-spacing-lg);
  background: var(--rf-color-bg-container);

  .section {
    margin-bottom: var(--rf-spacing-lg);

    .section-title {
      font-size: var(--rf-font-size-md);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
      margin-bottom: var(--rf-spacing-md);
    }
  }

  .model-row {
    display: flex;
    gap: var(--rf-spacing-md);

    .model-select {
      flex: 1.5;
    }

    .model-select-full {
      flex: 1;
    }

    .temperature-item {
      flex: 1;
    }
  }

  .temperature-control {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);
    width: 100%;

    &.compact {
      gap: var(--rf-spacing-sm);
    }

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

  :deep(.el-divider--horizontal) {
    margin: var(--rf-spacing-md) 0;
  }

  .tools-selector {
    width: 100%;

    .tool-option {
      .tool-label {
        font-weight: var(--rf-font-weight-medium);
        color: var(--rf-color-text-primary);
      }

      .tool-description {
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        margin-top: var(--rf-spacing-3xs);
      }
    }

    .tools-tags {
      display: flex;
      flex-wrap: wrap;
      gap: var(--rf-spacing-sm);

      :deep(.el-tag) {
        font-size: var(--rf-font-size-sm);
        padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
        background: var(--rf-color-primary-light-9);
        border-color: var(--rf-color-primary-light-7);
        color: var(--rf-color-primary);

        .el-tag__close {
          color: var(--rf-color-primary);

          &:hover {
            background-color: var(--rf-color-primary-light-7);
          }
        }
      }
    }

    .no-tools-hint {
      color: var(--rf-color-text-secondary);
      font-size: var(--rf-font-size-sm);
      font-style: italic;
      padding: var(--rf-spacing-sm) 0;
    }
  }

  :deep(.el-form-item) {
    margin-bottom: var(--rf-spacing-lg);
  }

  :deep(.el-form-item__label) {
    padding: 0;
    line-height: 1.4;
    margin-bottom: var(--rf-spacing-sm);
  }

  :deep(.el-textarea__inner) {
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: var(--rf-font-size-sm);
    line-height: 1.5;
  }
}

html.dark {
  .agent-config-panel {
    background-color: var(--rf-color-bg-container);

    .temperature-control .temperature-value {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>