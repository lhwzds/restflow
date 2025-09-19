<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElButton, ElInput, ElDialog, ElForm, ElFormItem, ElSelect, ElOption, ElSlider, ElMessage, ElRow, ElCol } from 'element-plus'
import { Plus, Search } from '@element-plus/icons-vue'
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
import type { AgentNode } from '@/types/generated/AgentNode'

const {
  searchQuery,
  selectedAgent,
  filteredAgents,
  loadAgents,
  selectAgent
} = useAgentsList()

const { createAgent, updateAgent, deleteAgent } = useAgentOperations()
// Panel resizing for split view - allows adjustable config/chat panel widths
const { panelWidth, startDragging } = useAgentPanelResize()

const showCreateDialog = ref(false)
const createForm = ref<AgentNode>({
  model: 'gpt-4.1',
  prompt: '',
  temperature: 0.7,
  api_key: null,
  tools: null
})
const createFormName = ref('')

const availableModels = [
  { label: 'GPT-4.1', value: 'gpt-4.1' },
  { label: 'Claude Sonnet 4', value: 'claude-sonnet-4' },
  { label: 'DeepSeek V3', value: 'deepseek-v3' },
]

onMounted(() => {
  loadAgents()
})

async function handleCreate() {
  if (!createFormName.value.trim()) {
    ElMessage.error('Please enter Agent name')
    return
  }
  if (!createForm.value.prompt.trim()) {
    ElMessage.error('Please enter System Prompt')
    return
  }

  try {
    await createAgent(createFormName.value, createForm.value)
    showCreateDialog.value = false

    createFormName.value = ''
    createForm.value = {
      model: 'gpt-4.1',
      prompt: '',
      temperature: 0.7,
      api_key: null,
      tools: null
    }

    await loadAgents()
  } catch (error) {
    // Handled by composable
  }
}

async function handleUpdate(id: string, updates: any) {
  try {
    await updateAgent(id, updates)
    await loadAgents()
  } catch (error) {
    // Handled by composable
  }
}

async function handleDelete(id: string) {
  try {
    await deleteAgent(id)
    selectAgent(null)
    await loadAgents()
  } catch (error) {
    // Handled by composable
  }
}

function backToList() {
  selectAgent(null)
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
      />
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
          :agent="selectedAgent"
          @update="handleUpdate"
          @delete="handleDelete"
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
              v-for="model in availableModels"
              :key="model.value"
              :label="model.label"
              :value="model.value"
            />
          </ElSelect>
        </ElFormItem>

        <ElFormItem label="Temperature">
          <ElSlider
            v-model="createForm.temperature"
            :min="0"
            :max="2"
            :step="0.1"
            :show-tooltip="true"
          />
        </ElFormItem>

        <ElFormItem label="System Prompt" required>
          <ElInput
            v-model="createForm.prompt"
            type="textarea"
            placeholder="Enter system prompt"
            :rows="6"
          />
        </ElFormItem>

        <ElFormItem label="API Key (optional)">
          <ElInput
            v-model="createForm.api_key"
            type="password"
            placeholder="Enter API Key"
            show-password
          />
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
      width: var(--rf-search-input-width);
    }

    .agents-grid {
      margin-top: 20px;

      /* Element Plus row layout fix */
      :deep(.el-row) {
        display: flex;
        flex-wrap: wrap;
        margin-left: -8px;
        margin-right: -8px;
      }

      .agent-col {
        margin-bottom: 16px;
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
      width: 4px;
      background: var(--rf-color-border-base);
      cursor: ew-resize;
      flex-shrink: 0;
      align-self: stretch;
      transition: background 0.2s;

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
