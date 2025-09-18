<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElButton, ElInput, ElDialog, ElForm, ElFormItem, ElSelect, ElOption, ElSlider, ElMessage, ElEmpty, ElRow, ElCol } from 'element-plus'
import { Plus, Search, ArrowLeft } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import AgentCard from '../components/agents/AgentCard.vue'
import AgentConfigPanel from '../components/agents/AgentConfigPanel.vue'
import AgentChatPanel from '../components/agents/AgentChatPanel.vue'
import { useAgentsList } from '../composables/agents/useAgentsList'
import { useAgentOperations } from '../composables/agents/useAgentOperations'
import { useAgentPanelResize } from '../composables/agents/useAgentPanelResize'
import type { AgentNode } from '@/types/generated/AgentNode'

// Composables
const {
  searchQuery,
  selectedAgent,
  filteredAgents,
  loadAgents,
  selectAgent
} = useAgentsList()

const { createAgent, updateAgent, deleteAgent } = useAgentOperations()
const { panelWidth, startDragging } = useAgentPanelResize()

// Create new Agent dialog
const showCreateDialog = ref(false)
const createForm = ref<AgentNode>({
  model: 'gpt-4.1',
  prompt: '',
  temperature: 0.7,
  api_key: null,
  tools: null
})
const createFormName = ref('')

// Available model list
const availableModels = [
  { label: 'GPT-4.1', value: 'gpt-4.1' },
  { label: 'Claude Sonnet 4', value: 'claude-sonnet-4' },
  { label: 'DeepSeek V3', value: 'deepseek-v3' },
]

// Load agents
onMounted(() => {
  loadAgents()
})

// Create new Agent
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

    // Reset form
    createFormName.value = ''
    createForm.value = {
      model: 'gpt-4.1',
      prompt: '',
      temperature: 0.7,
      api_key: null,
      tools: null
    }

    // Reload list
    await loadAgents()
  } catch (error) {
    // Error already handled in composable
  }
}

// Update Agent
async function handleUpdate(id: string, updates: any) {
  try {
    await updateAgent(id, updates)
    // Reload list
    await loadAgents()
  } catch (error) {
    // Error already handled in composable
  }
}

// Delete Agent
async function handleDelete(id: string) {
  try {
    await deleteAgent(id)
    selectAgent(null)
    // Reload list
    await loadAgents()
  } catch (error) {
    // Error already handled in composable
  }
}

// Back to list
function backToList() {
  selectAgent(null)
}
</script>

<template>
  <div class="agent-management">
    <!-- Card grid view (unselected state) -->
    <template v-if="!selectedAgent">
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

      <!-- Agents grid -->
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

      <!-- Empty state -->
      <div v-else class="empty-state">
        <ElEmpty
          :description="searchQuery ? 'No matching Agents found' : 'No Agents created yet'"
        >
          <ElButton
            v-if="!searchQuery"
            type="primary"
            @click="showCreateDialog = true"
          >
            Create First Agent
          </ElButton>
          <ElButton
            v-else
            @click="searchQuery = ''"
          >
            Clear Search
          </ElButton>
        </ElEmpty>
      </div>
    </template>

    <!-- Split screen view (selected state) -->
    <template v-else>
      <div class="detail-header">
        <ElButton
          :icon="ArrowLeft"
          @click="backToList"
        >
          Back to List
        </ElButton>
      </div>

      <div class="split-container">
        <!-- Left configuration panel -->
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

        <!-- Draggable splitter -->
        <div
          class="splitter"
          @mousedown="startDragging"
        />

        <!-- Right chat panel -->
        <div class="chat-panel">
          <AgentChatPanel :agent="selectedAgent" />
        </div>
      </div>
    </template>

    <!-- Create Agent Dialog -->
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
  </div>
</template>

<style lang="scss" scoped>
.agent-management {
  padding: 20px;
  height: 100%;
  overflow-y: auto;
  background-color: var(--rf-color-bg-page);
  box-sizing: border-box;
  overflow-x: hidden;

  .search-input {
    width: 300px;
  }

  // Card grid layout
  .agents-grid {
    margin-top: 20px;

    // Fix Element Plus row layout
    :deep(.el-row) {
      display: flex;
      flex-wrap: wrap;
      margin-left: -8px;
      margin-right: -8px;
    }

    // Ensure proper column spacing
    .agent-col {
      margin-bottom: 16px;
      display: flex;

      // Fix height issue by making card fill column
      .agent-card {
        width: 100%;
      }
    }
  }

  .empty-state {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 400px;
    margin-top: 20px;
  }

  // Detail view
  .detail-header {
    margin: -20px -20px 20px -20px;
    padding: 12px 20px;
    background: var(--rf-color-bg-container);
    border-bottom: 1px solid var(--rf-color-border-lighter);
  }

  .split-container {
    display: flex;
    height: calc(100vh - 200px);
    margin: 0 -20px;

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
    }
  }
}

// Dark mode adaptation
html.dark {
  .agent-management {
    .detail-header {
      background-color: var(--rf-color-bg-container);
    }

    .split-container {
      .config-panel {
        background-color: var(--rf-color-bg-container);
      }
    }
  }
}
</style>
