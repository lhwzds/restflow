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
  ElTag,
} from 'element-plus'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { getAllModels, supportsTemperature, getDefaultTemperature } from '@/utils/AIModels'
import { useAgentTools } from '@/composables/agents/useAgentTools'

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

const availableModels = computed(() => getAllModels())
const {
  selectedTools,
  selectedToolValue,
  addTool,
  removeTool,
  getToolLabel,
  getAvailableTools,
  resetTools,
} = useAgentTools(props.agent.agent.tools || [])

const formData = ref({
  name: props.agent.name,
  model: props.agent.agent.model,
  prompt: props.agent.agent.prompt,
  temperature:
    props.agent.agent.temperature !== undefined && props.agent.agent.temperature !== null
      ? props.agent.agent.temperature
      : getDefaultTemperature(props.agent.agent.model),
  api_key_config: props.agent.agent.api_key_config,
})

const keyMode = computed({
  get: () => formData.value.api_key_config?.type || 'direct',
  set: (value: 'direct' | 'secret') => {
    formData.value.api_key_config = buildConfig(value, '')
  },
})

const apiKeyValue = computed({
  get: () => formData.value.api_key_config?.value || '',
  set: (value: string) => {
    const mode = keyMode.value as 'direct' | 'secret'
    formData.value.api_key_config = buildConfig(mode, value)
  },
})

watch(
  () => props.agent,
  (newAgent) => {
    formData.value = {
      name: newAgent.name,
      model: newAgent.agent.model,
      prompt: newAgent.agent.prompt,
      temperature:
        newAgent.agent.temperature !== undefined && newAgent.agent.temperature !== null
          ? newAgent.agent.temperature
          : getDefaultTemperature(newAgent.agent.model),
      api_key_config: newAgent.agent.api_key_config,
    }
    resetTools(newAgent.agent.tools || [])
  },
  { deep: true },
)

onMounted(async () => {
  await loadSecretsData()
})

const isOSeriesModel = computed(() => !supportsTemperature(formData.value.model))

const hasChanges = computed(() => {
  return (
    formData.value.name !== props.agent.name ||
    formData.value.model !== props.agent.agent.model ||
    (formData.value.prompt || null) !== (props.agent.agent.prompt || null) ||
    formData.value.temperature !== props.agent.agent.temperature ||
    isConfigChanged(props.agent.agent.api_key_config, formData.value.api_key_config) ||
    JSON.stringify(selectedTools.value) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

watch(hasChanges, (newVal) => {
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
    JSON.stringify(selectedTools.value) !== JSON.stringify(props.agent.agent.tools || [])

  if (agentChanged) {
    updates.agent = {
      model: formData.value.model,
      prompt: formData.value.prompt?.trim() || null,
      temperature: isOSeriesModel.value ? null : formData.value.temperature,
      api_key_config: formData.value.api_key_config || null,
      tools: selectedTools.value.length > 0 ? selectedTools.value : null,
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
    temperature:
      props.agent.agent.temperature !== undefined && props.agent.agent.temperature !== null
        ? props.agent.agent.temperature
        : getDefaultTemperature(props.agent.agent.model),
    api_key_config: props.agent.agent.api_key_config,
  }
  resetTools(props.agent.agent.tools || [])
}

defineExpose({
  saveChanges,
  resetForm,
  hasChanges,
})
</script>

<template>
  <div class="agent-config-panel">
    <ElForm :model="formData" label-position="top">
      <div class="section">
        <h3 class="section-title">Basic Information</h3>
        <ElFormItem label="Agent Name" required>
          <ElInput v-model="formData.name" placeholder="Enter Agent name" clearable />
        </ElFormItem>
      </div>

      <ElDivider />

      <div class="section">
        <h3 class="section-title">Model Configuration</h3>
        <div class="model-row">
          <ElFormItem
            label="Model"
            :class="{ 'model-select': !isOSeriesModel, 'model-select-full': isOSeriesModel }"
          >
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
                v-model="formData.temperature!"
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
                v-for="tool in getAvailableTools()"
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

            <div v-if="selectedTools.length > 0" class="tools-tags">
              <ElTag
                v-for="toolValue in selectedTools"
                :key="toolValue"
                closable
                size="large"
                @close="removeTool(toolValue)"
              >
                {{ getToolLabel(toolValue) }}
              </ElTag>
            </div>
            <div v-else class="no-tools-hint">No tools selected</div>
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
