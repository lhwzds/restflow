<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { ElButton, ElInput, ElDialog, ElForm, ElFormItem, ElSelect, ElOption, ElSlider, ElMessage, ElRow, ElCol, ElRadioGroup, ElRadio, ElPopconfirm, ElTag } from 'element-plus'
import { Plus, Search, Delete, Check, RefreshLeft } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import PageHeader from '../components/shared/PageHeader.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import AgentCard from '../components/agents/AgentCard.vue'
import AgentConfigPanel from '../components/agents/AgentConfigPanel.vue'
import AgentChatPanel from '../components/agents/AgentChatPanel.vue'
import { useAgentsList } from '../composables/agents/useAgentsList'
import { useAgentOperations } from '../composables/agents/useAgentOperations'
import { useAgentPanelResize } from '../composables/agents/useAgentPanelResize'
import { useSecretsData } from '../composables/secrets/useSecretsData'
import type { AgentNode } from '@/types/generated/AgentNode'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { useAgentModels } from '@/composables/agents/useAgentModels'
import { useAgentTools } from '@/composables/agents/useAgentTools'

const {
  searchQuery,
  selectedAgent,
  filteredAgents,
  loadAgents,
  selectAgent
} = useAgentsList()

const { createAgent, updateAgent, deleteAgent } = useAgentOperations()
const { panelWidth, startDragging } = useAgentPanelResize()
const { secrets, loadSecrets: loadSecretsData } = useSecretsData()
const { buildConfig } = useApiKeyConfig()

// Use shared composables for models and tools
const { AVAILABLE_MODELS, isOSeriesModel: checkIsOSeriesModel, getDefaultTemperature } = useAgentModels()
const {
  selectedTools: createSelectedTools,
  selectedToolValue: createSelectedToolValue,
  addTool: addCreateTool,
  removeTool: removeCreateTool,
  getToolLabel,
  getAvailableTools,
  resetTools: resetCreateTools
} = useAgentTools()

const showCreateDialog = ref(false)
const createKeyMode = ref<'direct' | 'secret'>('direct')
const createApiKey = ref('')
const createApiKeySecret = ref('')
const createForm = ref<Omit<AgentNode, 'api_key_config'>>({
  model: 'gpt-4.1',
  prompt: null,
  temperature: 0.7,
  tools: null
})
const createFormName = ref('')

const isOSeriesModel = computed(() => checkIsOSeriesModel(createForm.value.model))


onMounted(async () => {
  loadAgents()
  await loadSecretsData()
})

async function handleCreate() {
  if (!createFormName.value.trim()) {
    ElMessage.error('Please enter Agent name')
    return
  }

  try {
    const apiKeyValue = createKeyMode.value === 'direct' ? createApiKey.value : createApiKeySecret.value
    const apiKeyConfig = buildConfig(createKeyMode.value, apiKeyValue)

    const agentData: AgentNode = {
      model: createForm.value.model,
      prompt: createForm.value.prompt?.trim() || null,
      temperature: isOSeriesModel.value ? null : createForm.value.temperature,
      api_key_config: apiKeyConfig,
      tools: createSelectedTools.value.length > 0 ? createSelectedTools.value : null
    }

    await createAgent(createFormName.value, agentData)
    showCreateDialog.value = false

    createFormName.value = ''
    createKeyMode.value = 'direct'
    createApiKey.value = ''
    createApiKeySecret.value = ''
    resetCreateTools([])
    createForm.value = {
      model: 'gpt-4.1',
      prompt: null,
      temperature: getDefaultTemperature('gpt-4.1') ?? 0.7,
      tools: null
    }

    await loadAgents()
  } catch (error) {
  }
}

async function handleUpdate(id: string, updates: any) {
  try {
    await updateAgent(id, updates)
    await loadAgents()
  } catch (error) {
  }
}

async function handleDelete(id: string) {
  try {
    await deleteAgent(id)
    selectAgent(null)
    await loadAgents()
  } catch (error) {
  }
}

function backToList() {
  selectAgent(null)
}

const hasAgentChanges = ref(false)
const agentConfigPanelRef = ref()

function handleSaveAgent() {
  agentConfigPanelRef.value?.saveChanges()
}

function handleResetAgent() {
  agentConfigPanelRef.value?.resetForm()
}

async function handleDeleteAgent() {
  if (!selectedAgent.value) return
  await handleDelete(selectedAgent.value.id)
}

function onAgentConfigChange(hasChanges: boolean) {
  hasAgentChanges.value = hasChanges
}
</script>

<template>
  <PageLayout :variant="selectedAgent ? 'split' : 'default'" :no-padding="!!selectedAgent">
    <template v-if="selectedAgent" #header>
      <PageHeader
        :title="selectedAgent.name"
        :subtitle="`${selectedAgent.agent.model} Agent`"
        :show-back="true"
        :back-to="backToList"
      >
        <template #actions>
          <ElButton
            type="primary"
            :icon="Check"
            :disabled="!hasAgentChanges"
            @click="handleSaveAgent"
          >
            Save Changes
          </ElButton>

          <ElButton
            v-if="hasAgentChanges"
            :icon="RefreshLeft"
            @click="handleResetAgent"
          >
            Reset
          </ElButton>

          <ElPopconfirm
            title="Are you sure you want to delete this Agent?"
            confirm-button-text="Confirm"
            cancel-button-text="Cancel"
            @confirm="handleDeleteAgent"
          >
            <template #reference>
              <ElButton
                type="danger"
                :icon="Delete"
              >
                Delete Agent
              </ElButton>
            </template>
          </ElPopconfirm>
        </template>
      </PageHeader>
    </template>

    <div class="agent-management">
      <div v-if="!selectedAgent" class="agent-management__list">
        <HeaderBar title="Agent Management">
        <template #actions>
          <ElInput
            v-model="searchQuery"
            placeholder="Search Agents..."
            :prefix-icon="Search"
            clearable
            class="search-input"
          />
          <ElButton
            type="primary"
            :icon="Plus"
            @click="showCreateDialog = true"
          >
            New Agent
          </ElButton>
        </template>
        </HeaderBar>

        <SearchInfo
          :count="filteredAgents.length"
          :search-query="searchQuery"
          item-name="agent"
          @clear="searchQuery = ''"
        />

        <div v-if="filteredAgents.length > 0" class="agents-grid">
        <ElRow :gutter="16">
          <ElCol
            v-for="agent in filteredAgents"
            :key="agent.id"
            :xs="24"
            :sm="12"
            :md="8"
            :lg="6"
            :xl="4"
            class="agent-col"
          >
            <AgentCard
              :agent="agent"
              @click="selectAgent"
            />
          </ElCol>
        </ElRow>
        </div>

        <EmptyState
          v-else
          :search-query="searchQuery"
          item-name="agent"
          create-text="Create First"
          @action="showCreateDialog = true"
          @clear-search="searchQuery = ''"
        />
      </div>

      <div v-else class="agent-management__detail">
        <div class="split-container">
      <div
        class="config-panel"
        :style="{ width: `${panelWidth}px` }"
      >
        <AgentConfigPanel
          ref="agentConfigPanelRef"
          :agent="selectedAgent"
          @update="handleUpdate"
          @delete="handleDelete"
          @changes-update="onAgentConfigChange"
        />
      </div>

      <div
        class="splitter"
        @mousedown="startDragging"
      />

        <div class="chat-panel">
          <AgentChatPanel :agent="selectedAgent" />
        </div>
      </div>
      </div>
    </div>

    <ElDialog
      v-model="showCreateDialog"
      title="Create New Agent"
      width="600px"
    >
      <ElForm :model="createForm" label-position="top">
        <ElFormItem label="Agent Name" required>
          <ElInput
            v-model="createFormName"
            placeholder="Enter Agent name"
          />
        </ElFormItem>

        <ElFormItem label="Model" required>
          <ElSelect v-model="createForm.model" placeholder="Select model">
            <ElOption
              v-for="model in AVAILABLE_MODELS"
              :key="model.value"
              :label="model.label"
              :value="model.value"
            />
          </ElSelect>
        </ElFormItem>

        <ElFormItem v-if="!isOSeriesModel" label="Temperature">
          <ElSlider
            v-model="createForm.temperature!"
            :min="0"
            :max="2"
            :step="0.1"
            :show-tooltip="true"
          />
        </ElFormItem>

        <ElFormItem label="System Prompt (Optional)">
          <ElInput
            v-model="createForm.prompt"
            type="textarea"
            placeholder="Enter system prompt (optional)"
            :rows="4"
          />
        </ElFormItem>

        <ElFormItem label="Tools Configuration (Optional)">
          <div class="tools-selector">
            <ElSelect
              v-model="createSelectedToolValue"
              placeholder="Select a tool to add"
              clearable
              @change="addCreateTool"
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

            <div v-if="createSelectedTools.length > 0" class="tools-tags">
              <ElTag
                v-for="toolValue in createSelectedTools"
                :key="toolValue"
                closable
                size="large"
                @close="removeCreateTool(toolValue)"
              >
                {{ getToolLabel(toolValue) }}
              </ElTag>
            </div>
            <div v-else class="no-tools-hint">
              No tools selected
            </div>
          </div>
        </ElFormItem>

        <ElFormItem label="API Key Configuration">
          <ElRadioGroup v-model="createKeyMode" style="margin-bottom: var(--rf-spacing-md)">
            <ElRadio value="direct">Direct Input</ElRadio>
            <ElRadio value="secret">Use Secret Manager</ElRadio>
          </ElRadioGroup>

          <ElInput
            v-if="createKeyMode === 'direct'"
            v-model="createApiKey"
            type="password"
            placeholder="Enter API Key (optional)"
            show-password
          />

          <ElSelect
            v-else
            v-model="createApiKeySecret"
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
      </ElForm>

      <template #footer>
        <ElButton @click="showCreateDialog = false">Cancel</ElButton>
        <ElButton type="primary" @click="handleCreate">Create</ElButton>
      </template>
    </ElDialog>
  </PageLayout>
</template>

<style lang="scss" scoped>
.agent-management {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;

  &__list {
    .search-input {
      width: var(--rf-size-xl);
    }

    .agents-grid {
      margin-top: var(--rf-spacing-xl);

      :deep(.el-row) {
        display: flex;
        flex-wrap: wrap;
        margin-left: -8px;
        margin-right: -8px;
      }

      .agent-col {
        margin-bottom: var(--rf-spacing-lg);
        display: flex;

        .agent-card {
          width: 100%;
        }
      }
    }

  }

  &__detail {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .split-container {
    display: flex;
    flex: 1;
    min-height: 0;
    align-items: stretch;

    .config-panel {
      background: var(--rf-color-bg-container);
      overflow-y: auto;
      flex-shrink: 0;
    }

    .splitter {
      width: var(--rf-size-splitter);
      background: var(--rf-color-border-base);
      cursor: ew-resize;
      flex-shrink: 0;
      align-self: stretch;
      transition: background var(--rf-transition-fast);

      &:hover {
        background: var(--rf-color-primary);
      }

      &:active {
        background: var(--rf-color-primary);
      }
    }

    .chat-panel {
      flex: 1;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
    }
  }
}


html.dark {
  .agent-management {
    .split-container {
      .config-panel {
        background-color: var(--rf-color-bg-container);
      }
    }
  }
}
</style>
