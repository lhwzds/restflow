<script setup lang="ts">
import { Plus, Search } from '@element-plus/icons-vue'
import { Lightbulb } from 'lucide-vue-next'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import WorkflowCard from '../components/workflow-list/WorkflowCard.vue'
import WorkflowEmptyState from '../components/workflow-list/WorkflowEmptyState.vue'
import NewWorkflowDialog from '../components/workflow-list/NewWorkflowDialog.vue'
import { ElButton, ElCol, ElInput, ElRow } from 'element-plus'
import { onMounted, ref } from 'vue'
import { useWorkflowList } from '../composables/list/useWorkflowList'
import { useWorkflowListSelection } from '../composables/list/useWorkflowListSelection'
import { useWorkflowTriggers } from '../composables/triggers/useWorkflowTriggers'
import { isNodeATrigger } from '../composables/node/useNodeHelpers'
import type { Workflow } from '@/types/generated/Workflow'

const {
  workflows,
  isLoading,
  filteredWorkflows,
  searchQuery,
  loadWorkflows,
  setSearchQuery,
} = useWorkflowList()

const {
  fetchAllTriggerStatuses,
  getTriggerStatus,
} = useWorkflowTriggers()

const { selectedWorkflowId, selectWorkflow, clearSelection } = useWorkflowListSelection(workflows)

const showNewWorkflowDialog = ref(false)

onMounted(async () => {
  await loadWorkflows()
  await fetchAllTriggerStatuses(workflows.value.map((w) => w.id))
})


function createWorkflow() {
  showNewWorkflowDialog.value = true
}

function handleSearch(value: string) {
  setSearchQuery(value)
}

async function handleWorkflowUpdated() {
  await loadWorkflows()
  await fetchAllTriggerStatuses(workflows.value.map((w) => w.id))
}

async function handleWorkflowDeleted(workflowId: string) {
  if (selectedWorkflowId.value === workflowId) {
    clearSelection()
  }
  await loadWorkflows()
}

function clearSearch() {
  searchQuery.value = ''
}

function hasTrigger(workflow: Workflow): boolean {
  return workflow.nodes?.some(isNodeATrigger) ?? false
}
</script>

<template>
  <PageLayout variant="default">
    <HeaderBar title="Workflows">
      <template #actions>
        <ElInput
          v-model="searchQuery"
          placeholder="Search workflow name or description..."
          :prefix-icon="Search"
          clearable
          class="search-input"
          @input="handleSearch"
        />
        <ElButton type="primary" :icon="Plus" @click="createWorkflow">
          New Workflow
        </ElButton>
      </template>
    </HeaderBar>

    <div v-if="searchQuery" class="search-info">
      <span>Found {{ filteredWorkflows.length }} workflow(s) matching "{{ searchQuery }}"</span>
      <ElButton link @click="clearSearch">Clear</ElButton>
    </div>

    <WorkflowEmptyState
      v-if="filteredWorkflows.length === 0"
      :search-query="searchQuery"
      :is-loading="isLoading"
      @create-workflow="createWorkflow"
      @clear-search="clearSearch"
    />

    <div v-else class="workflow-grid-container">
      <div class="help-text">
        <Lightbulb :size="14" class="help-icon" />
        <span>
          Tip: Click to select • Double-click to open • Ctrl+C/V to copy/paste • Click to rename
        </span>
      </div>

      <ElRow :gutter="20" class="workflow-grid">
        <ElCol v-for="workflow in filteredWorkflows" :key="workflow.id" :span="8">
          <WorkflowCard
            :workflow="workflow"
            :is-selected="selectedWorkflowId === workflow.id"
            :is-active="getTriggerStatus(workflow.id)?.is_active"
            :has-trigger="hasTrigger(workflow)"
            @select="selectWorkflow(workflow.id)"
            @updated="handleWorkflowUpdated"
            @deleted="handleWorkflowDeleted"
          />
        </ElCol>
      </ElRow>
    </div>

    <NewWorkflowDialog v-model:visible="showNewWorkflowDialog" />
  </PageLayout>
</template>

<style lang="scss" scoped>
.search-input {
  width: 300px;
}

.search-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background-color: var(--rf-color-bg-overlay);
  border-radius: 4px;
  margin-bottom: 20px;
  font-size: 14px;
  color: var(--rf-color-text-regular);
}

.workflow-grid-container {
  width: 100%;
}

.help-text {
  margin-bottom: 20px;
  padding: 10px 16px;
  background: linear-gradient(
    135deg,
    var(--rf-color-primary-bg-lighter) 0%,
    var(--rf-color-primary-bg-light) 100%
  );
  border: 1px solid var(--rf-color-primary-bg);
  border-radius: 6px;
  text-align: center;
  font-size: 13px;
  color: var(--rf-color-primary);
  animation: fadeIn 0.3s ease-in;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;

  .help-icon {
    flex-shrink: 0;
  }
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(-10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.workflow-grid {
  margin-top: 20px;
}
</style>
