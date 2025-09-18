<script setup lang="ts">
import { ref, computed, watch } from 'vue'
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
  ElMessage
} from 'element-plus'
import { Check, Delete, CopyDocument } from '@element-plus/icons-vue'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'

const props = defineProps<{
  agent: StoredAgent
}>()

const emit = defineEmits<{
  update: [id: string, updates: { name?: string; agent?: AgentNode }]
  delete: [id: string]
}>()

// Form data
const formData = ref({
  name: props.agent.name,
  model: props.agent.agent.model,
  prompt: props.agent.agent.prompt,
  temperature: props.agent.agent.temperature,
  api_key: props.agent.agent.api_key || '',
  tools: props.agent.agent.tools || []
})

// Watch props changes, update form data
watch(() => props.agent, (newAgent) => {
  formData.value = {
    name: newAgent.name,
    model: newAgent.agent.model,
    prompt: newAgent.agent.prompt,
    temperature: newAgent.agent.temperature,
    api_key: newAgent.agent.api_key || '',
    tools: newAgent.agent.tools || []
  }
}, { deep: true })

// Available model list
const availableModels = [
  { label: 'GPT-4.1', value: 'gpt-4.1' },
  { label: 'Claude Sonnet 4', value: 'claude-sonnet-4' },
  { label: 'DeepSeek V3', value: 'deepseek-v3' },
]

// Available tools list
const availableTools = [
  { label: 'Addition Calculator', value: 'add' },
  { label: 'Get Current Time', value: 'get_current_time' }
]

// Has unsaved changes
const hasChanges = computed(() => {
  return (
    formData.value.name !== props.agent.name ||
    formData.value.model !== props.agent.agent.model ||
    formData.value.prompt !== props.agent.agent.prompt ||
    formData.value.temperature !== props.agent.agent.temperature ||
    formData.value.api_key !== (props.agent.agent.api_key || '') ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])
  )
})

// Save changes
function handleSave() {
  if (!formData.value.name.trim()) {
    ElMessage.error('Agent name cannot be empty')
    return
  }

  const updates: { name?: string; agent?: AgentNode } = {}

  if (formData.value.name !== props.agent.name) {
    updates.name = formData.value.name
  }

  // Check if agent configuration has changed
  const agentChanged =
    formData.value.model !== props.agent.agent.model ||
    formData.value.prompt !== props.agent.agent.prompt ||
    formData.value.temperature !== props.agent.agent.temperature ||
    formData.value.api_key !== (props.agent.agent.api_key || '') ||
    JSON.stringify(formData.value.tools) !== JSON.stringify(props.agent.agent.tools || [])

  if (agentChanged) {
    updates.agent = {
      model: formData.value.model,
      prompt: formData.value.prompt,
      temperature: formData.value.temperature,
      api_key: formData.value.api_key || null,
      tools: formData.value.tools.length > 0 ? formData.value.tools : null
    }
  }

  emit('update', props.agent.id, updates)
}

// Delete Agent
function handleDelete() {
  emit('delete', props.agent.id)
}

// Copy configuration
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

// Reset form
function resetForm() {
  formData.value = {
    name: props.agent.name,
    model: props.agent.agent.model,
    prompt: props.agent.agent.prompt,
    temperature: props.agent.agent.temperature,
    api_key: props.agent.agent.api_key || '',
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

        <ElFormItem label="API Key">
          <ElInput
            v-model="formData.api_key"
            type="password"
            placeholder="Enter API Key (optional)"
            show-password
            clearable
          />
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
  padding: 20px;
  background: var(--rf-color-bg-container);

  .section {
    margin-bottom: 20px;

    .section-title {
      font-size: 16px;
      font-weight: 600;
      color: var(--rf-color-text-primary);
      margin-bottom: 16px;
    }
  }

  .temperature-control {
    display: flex;
    align-items: center;
    gap: 20px;
    width: 100%;

    :deep(.el-slider) {
      flex: 1;
    }

    .temperature-value {
      min-width: 40px;
      text-align: center;
      font-weight: 600;
      color: var(--rf-color-primary);
      background: var(--rf-color-bg-secondary);
      padding: 4px 8px;
      border-radius: 4px;
    }
  }

  .actions {
    display: flex;
    gap: 12px;
    margin-top: 24px;
  }

  :deep(.el-divider--horizontal) {
    margin: 20px 0;
  }

  :deep(.el-checkbox-group) {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  :deep(.el-textarea__inner) {
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: 13px;
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