<script setup lang="ts">
import { computed, onMounted, toRef } from 'vue'
import { useRoute } from 'vue-router'
import {
  ElButton,
  ElInput,
  ElSelect,
  ElOption,
  ElSlider,
  ElRadioGroup,
  ElRadio,
  ElTag,
  ElSkeleton,
  ElDivider,
} from 'element-plus'
import { ArrowLeft, Delete, RefreshLeft } from '@element-plus/icons-vue'
import PageLayout from '@/components/shared/PageLayout.vue'
import AgentChatPanel from '@/components/agents/AgentChatPanel.vue'
import { useAgentEditor } from '@/composables/agents/useAgentEditor'
import { useAgentTools } from '@/composables/agents/useAgentTools'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { getAllModels, supportsTemperature, getModelDisplayName } from '@/utils/AIModels'

const route = useRoute()
const agentId = route.params.id as string

const {
  agent,
  formData,
  isLoading,
  isSaving,
  error,
  hasChanges,
  loadAgent,
  saveAgent,
  handleDelete,
  resetForm,
  goBack,
} = useAgentEditor(agentId)

const { secrets, loadSecrets } = useSecretsData()
const { buildConfig } = useApiKeyConfig()
const availableModels = computed(() => getAllModels())

// Pass formData.tools directly - single source of truth, no sync needed
const {
  selectedToolValue,
  isLoading: isLoadingTools,
  error: toolsError,
  addTool,
  removeTool,
  getToolLabel,
  getAvailableTools,
  loadTools,
} = useAgentTools(toRef(() => formData.value.tools))

// API Key mode handling
const keyMode = computed({
  get: () => formData.value.api_key_config?.type || 'direct',
  set: (newMode: 'direct' | 'secret') => {
    // Preserve existing value when switching modes, or use empty string
    const currentValue = formData.value.api_key_config?.value || ''
    // Set config even with empty value to preserve mode selection
    formData.value.api_key_config = {
      type: newMode,
      value: currentValue,
    }
  },
})

const apiKeyValue = computed({
  get: () => formData.value.api_key_config?.value || '',
  set: (value: string) => {
    const mode = keyMode.value as 'direct' | 'secret'
    if (!value || !value.trim()) {
      // Keep mode but clear value
      formData.value.api_key_config = {
        type: mode,
        value: '',
      }
    } else {
      formData.value.api_key_config = buildConfig(mode, value)
    }
  },
})

const isOSeriesModel = computed(() => !supportsTemperature(formData.value.model))

// Temperature value with null handling for slider
const temperatureValue = computed({
  get: () => formData.value.temperature ?? 0.7,
  set: (value: number) => {
    formData.value.temperature = value
  },
})

// Create a computed StoredAgent for the chat panel
const agentForChat = computed(() => {
  if (!agent.value) return null
  return {
    ...agent.value,
    name: formData.value.name,
    agent: {
      model: formData.value.model,
      prompt: formData.value.prompt,
      temperature: formData.value.temperature,
      api_key_config: formData.value.api_key_config,
      tools: formData.value.tools.length > 0 ? formData.value.tools : null,
    },
  }
})

onMounted(async () => {
  try {
    await Promise.all([loadAgent(), loadSecrets(), loadTools()])
  } catch (err) {
    console.error('Failed to initialize agent editor:', err)
  }
})
</script>

<template>
  <PageLayout class="agent-editor-page" variant="fullheight" no-padding>
    <!-- Header -->
    <div class="editor-header">
      <div class="header-left">
        <ElButton text :icon="ArrowLeft" @click="goBack">Back</ElButton>
        <div class="title-section">
          <ElInput
            v-model="formData.name"
            class="title-input"
            placeholder="Agent name"
            :disabled="isLoading"
          />
          <ElTag v-if="agent" type="info" size="small" class="model-tag">
            {{ getModelDisplayName(formData.model) }}
          </ElTag>
        </div>
      </div>
      <div class="header-actions">
        <ElButton
          v-if="hasChanges"
          text
          :icon="RefreshLeft"
          @click="resetForm"
          :disabled="isLoading"
        >
          Reset
        </ElButton>
        <ElButton text type="danger" :icon="Delete" @click="handleDelete" :disabled="isLoading">
          Delete
        </ElButton>
        <ElButton
          type="primary"
          @click="saveAgent"
          :loading="isSaving"
          :disabled="!hasChanges || isLoading"
        >
          Save
        </ElButton>
      </div>
    </div>

    <!-- Main content -->
    <div class="editor-main">
      <ElSkeleton v-if="isLoading" :rows="10" animated />

      <div v-else-if="error" class="error-state">
        <p>{{ error }}</p>
        <ElButton @click="loadAgent">Retry</ElButton>
      </div>

      <div v-else-if="agent" class="split-editor">
        <!-- Config pane -->
        <div class="config-pane">
          <div class="pane-header">
            <span>Configuration</span>
          </div>
          <div class="pane-content">
            <!-- Model Configuration -->
            <div class="config-section">
              <h4 class="section-title">Model</h4>
              <div class="model-row">
                <div class="model-select-wrapper">
                  <ElSelect v-model="formData.model" placeholder="Select model">
                    <ElOption
                      v-for="model in availableModels"
                      :key="model.value"
                      :label="model.label"
                      :value="model.value"
                    />
                  </ElSelect>
                </div>
                <div v-if="!isOSeriesModel" class="temperature-wrapper">
                  <span class="temp-label">Temperature</span>
                  <ElSlider
                    v-model="temperatureValue"
                    :min="0"
                    :max="2"
                    :step="0.1"
                    :show-tooltip="true"
                  />
                  <span class="temp-value">{{ temperatureValue }}</span>
                </div>
              </div>
            </div>

            <ElDivider />

            <!-- API Key Configuration -->
            <div class="config-section">
              <h4 class="section-title">API Key</h4>
              <ElRadioGroup v-model="keyMode" class="key-mode-group">
                <ElRadio value="direct">Direct Input</ElRadio>
                <ElRadio value="secret">Use Secret</ElRadio>
              </ElRadioGroup>

              <ElInput
                v-if="keyMode === 'direct'"
                v-model="apiKeyValue"
                type="password"
                placeholder="Enter API Key"
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
            </div>

            <ElDivider />

            <!-- System Prompt -->
            <div class="config-section">
              <h4 class="section-title">System Prompt</h4>
              <ElInput
                v-model="formData.prompt"
                type="textarea"
                placeholder="Enter system prompt (optional)"
                :autosize="{ minRows: 3, maxRows: 8 }"
              />
            </div>

            <ElDivider />

            <!-- Tools Configuration -->
            <div class="config-section">
              <h4 class="section-title">Tools</h4>
              <div v-if="toolsError" class="tools-error">
                Failed to load tools: {{ toolsError }}
              </div>
              <ElSelect
                v-model="selectedToolValue"
                placeholder="Select a tool to add"
                clearable
                :loading="isLoadingTools"
                @change="addTool"
                style="width: 100%; margin-bottom: var(--rf-spacing-sm)"
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

              <div v-if="formData.tools.length > 0" class="tools-tags">
                <ElTag
                  v-for="toolValue in formData.tools"
                  :key="toolValue"
                  closable
                  @close="removeTool(toolValue)"
                >
                  {{ getToolLabel(toolValue) }}
                </ElTag>
              </div>
              <div v-else class="no-tools-hint">No tools selected</div>
            </div>
          </div>
        </div>

        <!-- Divider -->
        <div class="divider" />

        <!-- Chat pane -->
        <div class="chat-pane">
          <AgentChatPanel v-if="agentForChat" :agent="agentForChat" />
        </div>
      </div>
    </div>
  </PageLayout>
</template>

<style lang="scss" scoped>
.agent-editor-page {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--rf-color-bg-base);
}

.editor-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--rf-spacing-sm) var(--rf-spacing-lg);
  border-bottom: 1px solid var(--rf-color-border-base);
  background: var(--rf-color-bg-container);

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);
    flex: 1;

    .title-section {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-md);
      flex: 1;
      max-width: 500px;

      .title-input {
        flex: 1;

        :deep(.el-input__wrapper) {
          box-shadow: none;
          background: transparent;
        }

        :deep(.el-input__inner) {
          font-size: var(--rf-font-size-lg);
          font-weight: var(--rf-font-weight-semibold);
        }
      }

      .model-tag {
        flex-shrink: 0;
      }
    }
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);
  }
}

.editor-main {
  flex: 1;
  display: flex;
  overflow: hidden;

  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    gap: var(--rf-spacing-md);
    color: var(--rf-color-text-secondary);
  }
}

.split-editor {
  display: flex;
  width: 100%;
  height: 100%;

  .config-pane {
    width: 400px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--rf-color-bg-container);
    border-right: 1px solid var(--rf-color-border-lighter);
    overflow: hidden;
  }

  .chat-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .pane-header {
    padding: var(--rf-spacing-sm) var(--rf-spacing-md);
    border-bottom: 1px solid var(--rf-color-border-lighter);
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-medium);
    color: var(--rf-color-text-secondary);
    background: var(--rf-color-bg-secondary);
  }

  .pane-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--rf-spacing-md);
  }

  .divider {
    width: 1px;
    background: var(--rf-color-border-lighter);
  }
}

.config-section {
  margin-bottom: var(--rf-spacing-md);

  .section-title {
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-semibold);
    color: var(--rf-color-text-primary);
    margin-bottom: var(--rf-spacing-sm);
  }

  .model-row {
    display: flex;
    flex-direction: column;
    gap: var(--rf-spacing-md);

    .model-select-wrapper {
      :deep(.el-select) {
        width: 100%;
      }
    }

    .temperature-wrapper {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-sm);

      .temp-label {
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        flex-shrink: 0;
      }

      :deep(.el-slider) {
        flex: 1;
      }

      .temp-value {
        min-width: 32px;
        text-align: center;
        font-weight: var(--rf-font-weight-semibold);
        color: var(--rf-color-primary);
        font-size: var(--rf-font-size-sm);
      }
    }
  }

  .key-mode-group {
    margin-bottom: var(--rf-spacing-sm);
  }

  .tools-tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rf-spacing-xs);
  }

  .no-tools-hint {
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-text-placeholder);
  }

  .tools-error {
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-danger);
    margin-bottom: var(--rf-spacing-sm);
  }

  .tool-option {
    .tool-label {
      font-weight: var(--rf-font-weight-medium);
    }
    .tool-description {
      font-size: var(--rf-font-size-xs);
      color: var(--rf-color-text-secondary);
    }
  }
}

:deep(.el-divider--horizontal) {
  margin: var(--rf-spacing-md) 0;
}

html.dark {
  .editor-header {
    background: var(--rf-color-bg-container);
  }

  .split-editor {
    .config-pane {
      background: var(--rf-color-bg-container);
    }

    .pane-header {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>
