<script setup lang="ts">
import { computed, onMounted, toRef } from 'vue'
import { useRoute } from 'vue-router'
import { ArrowLeft, Trash2, RotateCcw, X } from 'lucide-vue-next'
import PageLayout from '@/components/shared/PageLayout.vue'
import AgentChatPanel from '@/components/agents/AgentChatPanel.vue'
import { useAgentEditor } from '@/composables/agents/useAgentEditor'
import { useAgentTools } from '@/composables/agents/useAgentTools'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { getAllModels, supportsTemperature } from '@/utils/AIModels'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { Separator } from '@/components/ui/separator'
import { Label } from '@/components/ui/label'

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
      tools: formData.value.tools.length > 0 ? formData.value.tools : undefined,
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
        <Button variant="ghost" @click="goBack">
          <ArrowLeft class="mr-2 h-4 w-4" />
          Back
        </Button>
        <div class="title-section">
          <Input
            v-model="formData.name"
            class="title-input"
            placeholder="Agent name"
            :disabled="isLoading"
          />
        </div>
      </div>
      <div class="header-actions">
        <Button
          v-if="hasChanges"
          variant="ghost"
          @click="resetForm"
          :disabled="isLoading"
        >
          <RotateCcw class="mr-2 h-4 w-4" />
          Reset
        </Button>
        <Button variant="ghost" class="text-destructive" @click="handleDelete" :disabled="isLoading">
          <Trash2 class="mr-2 h-4 w-4" />
          Delete
        </Button>
        <Button
          @click="saveAgent"
          :disabled="!hasChanges || isLoading || isSaving"
        >
          {{ isSaving ? 'Saving...' : 'Save' }}
        </Button>
      </div>
    </div>

    <!-- Main content -->
    <div class="editor-main">
      <div v-if="isLoading" class="loading-state">
        <Skeleton class="h-8 w-full mb-4" />
        <Skeleton class="h-4 w-3/4 mb-2" />
        <Skeleton class="h-4 w-1/2 mb-2" />
        <Skeleton class="h-32 w-full mb-4" />
        <Skeleton class="h-4 w-2/3 mb-2" />
        <Skeleton class="h-4 w-1/2" />
      </div>

      <div v-else-if="error" class="error-state">
        <p>{{ error }}</p>
        <Button @click="loadAgent">Retry</Button>
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
                  <Select v-model="formData.model">
                    <SelectTrigger>
                      <SelectValue placeholder="Select model" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem
                        v-for="model in availableModels"
                        :key="model.value"
                        :value="model.value"
                      >
                        {{ model.label }}
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div v-if="!isOSeriesModel" class="temperature-wrapper">
                  <span class="temp-label">Temperature</span>
                  <Slider
                    :model-value="[temperatureValue]"
                    @update:model-value="(v) => { if (v && v[0] !== undefined) temperatureValue = v[0] }"
                    :min="0"
                    :max="2"
                    :step="0.1"
                    class="temperature-slider"
                  />
                  <span class="temp-value">{{ temperatureValue.toFixed(1) }}</span>
                </div>
              </div>
            </div>

            <Separator />

            <!-- API Key Configuration -->
            <div class="config-section">
              <h4 class="section-title">API Key</h4>
              <RadioGroup v-model="keyMode" class="key-mode-group">
                <div class="radio-option">
                  <RadioGroupItem id="direct" value="direct" />
                  <Label for="direct">Direct Input</Label>
                </div>
                <div class="radio-option">
                  <RadioGroupItem id="secret" value="secret" />
                  <Label for="secret">Use Secret</Label>
                </div>
              </RadioGroup>

              <Input
                v-if="keyMode === 'direct'"
                v-model="apiKeyValue"
                type="password"
                placeholder="Enter API Key"
              />
              <Select
                v-else
                v-model="apiKeyValue"
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select a secret" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="secret in secrets"
                    :key="secret.key"
                    :value="secret.key"
                  >
                    {{ secret.description || secret.key }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            <Separator />

            <!-- System Prompt -->
            <div class="config-section">
              <h4 class="section-title">System Prompt</h4>
              <Textarea
                v-model="formData.prompt"
                placeholder="Enter system prompt (optional)"
                class="prompt-textarea"
              />
            </div>

            <Separator />

            <!-- Tools Configuration -->
            <div class="config-section">
              <h4 class="section-title">Tools</h4>
              <div v-if="toolsError" class="tools-error">
                Failed to load tools: {{ toolsError }}
              </div>
              <Select
                v-model="selectedToolValue"
                @update:model-value="addTool"
              >
                <SelectTrigger class="tools-select-trigger">
                  <SelectValue placeholder="Select a tool to add" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="tool in getAvailableTools()"
                    :key="tool.value"
                    :value="tool.value"
                  >
                    <div class="tool-option">
                      <div class="tool-label">{{ tool.label }}</div>
                      <div class="tool-description">{{ tool.description }}</div>
                    </div>
                  </SelectItem>
                </SelectContent>
              </Select>

              <div v-if="formData.tools.length > 0" class="tools-tags">
                <Badge
                  v-for="toolValue in formData.tools"
                  :key="toolValue"
                  variant="secondary"
                  class="tool-badge"
                >
                  {{ getToolLabel(toolValue) }}
                  <button
                    type="button"
                    class="tool-remove-btn"
                    @click="removeTool(toolValue)"
                  >
                    <X :size="12" />
                  </button>
                </Badge>
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
        font-size: var(--rf-font-size-lg);
        font-weight: var(--rf-font-weight-semibold);
        border: none;
        background: transparent;
        box-shadow: none;

        &:focus {
          box-shadow: none;
          border: none;
        }
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

  .loading-state {
    width: 100%;
    padding: var(--rf-spacing-xl);
  }

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
      width: 100%;
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

      .temperature-slider {
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
    display: flex;
    gap: var(--rf-spacing-md);
    margin-bottom: var(--rf-spacing-sm);
  }

  .radio-option {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
  }

  .prompt-textarea {
    min-height: 80px;
    resize: vertical;
  }

  .tools-select-trigger {
    margin-bottom: var(--rf-spacing-sm);
  }

  .tools-tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rf-spacing-xs);
  }

  .tool-badge {
    display: inline-flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
    padding-right: var(--rf-spacing-xs);
  }

  .tool-remove-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    color: inherit;
    opacity: 0.7;
    border-radius: 50%;

    &:hover {
      opacity: 1;
      background: rgba(0, 0, 0, 0.1);
    }
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
