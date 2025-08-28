<script setup lang="ts">
import { Close, EditPen, Plus, Search, VideoPause, VideoPlay } from '@element-plus/icons-vue'
import {
  ElButton,
  ElCard,
  ElCol,
  ElDialog,
  ElEmpty,
  ElForm,
  ElFormItem,
  ElInput,
  ElMessage,
  ElRow,
  ElTooltip,
} from 'element-plus'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useWorkflowList } from '../composables/workflow/useWorkflowList'
import { useWorkflowTriggers } from '../composables/workflow/useWorkflowTriggers'
import { isNodeATrigger } from '../composables/node/useNodeHelpers'

const router = useRouter()

const {
  workflows,
  isLoading,
  filteredWorkflows,
  searchQuery,
  loadWorkflows,
  deleteWorkflow,
  duplicateWorkflow,
  renameWorkflow,
  setSearchQuery,
} = useWorkflowList()

const {
  triggerStatusMap,
  fetchAllTriggerStatuses,
  getTriggerStatus,
  activateTrigger,
  deactivateTrigger,
} = useWorkflowTriggers()

const newWorkflowName = ref('')
const dialogVisible = ref(false)
const editingWorkflowId = ref<string | null>(null)
const editingName = ref('')
const selectedWorkflowId = ref<string | null>(null)
const copiedWorkflow = ref<any>(null)

const handleKeyDown = (event: KeyboardEvent) => {
  // Ctrl+C or Cmd+C to copy
  if ((event.ctrlKey || event.metaKey) && event.key === 'c' && selectedWorkflowId.value) {
    const workflow = workflows.value.find((w) => w.id === selectedWorkflowId.value)
    if (workflow) {
      copiedWorkflow.value = workflow
      ElMessage.success('Workflow copied to clipboard')
    }
  }

  // Ctrl+V or Cmd+V to paste
  if ((event.ctrlKey || event.metaKey) && event.key === 'v' && copiedWorkflow.value) {
    duplicateWorkflow(copiedWorkflow.value.id, `${copiedWorkflow.value.name} (Copy)`)
  }

  // Escape to deselect
  if (event.key === 'Escape') {
    selectedWorkflowId.value = null
  }
}

onMounted(async () => {
  await loadWorkflows()
  // Batch fetch trigger status for all workflows
  await fetchAllTriggerStatuses(workflows.value.map((w) => w.id))

  // Add keyboard event listener
  document.addEventListener('keydown', handleKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeyDown)
})

const displayWorkflows = computed(() => {
  return filteredWorkflows.value.map((w) => ({
    ...w,
    createdAt: w.created_at || new Date().toISOString(),
    updatedAt: w.updated_at || new Date().toISOString(),
    nodeCount: w.nodes?.length || 0, // Get actual node count from workflow data
  }))
})

const createWorkflow = () => {
  newWorkflowName.value = ''
  dialogVisible.value = true
}

const saveWorkflow = () => {
  if (!newWorkflowName.value?.trim()) {
    ElMessage.error('Please enter a workflow name')
    return
  }
  // Create new workflow and navigate to editor
  router.push(`/workflow?name=${encodeURIComponent(newWorkflowName.value)}`)
  dialogVisible.value = false
}

const startRename = (workflow: any, event: MouseEvent) => {
  event.stopPropagation()
  event.preventDefault()
  editingWorkflowId.value = workflow.id
  editingName.value = workflow.name
  // Focus the input after Vue updates the DOM
  setTimeout(() => {
    const input = document.querySelector(`#rename-input-${workflow.id}`) as HTMLInputElement
    if (input) {
      input.focus()
      input.select()
    }
  }, 50)
}

const saveRename = async (workflowId: string) => {
  if (!editingName.value?.trim()) {
    ElMessage.error('Please enter a workflow name')
    return
  }

  const result = await renameWorkflow(workflowId, editingName.value)
  if (result.success) {
    editingWorkflowId.value = null
  }
}

const cancelRename = () => {
  editingWorkflowId.value = null
  editingName.value = ''
}

const handleRenameKeydown = (event: KeyboardEvent, workflowId: string) => {
  if (event.key === 'Enter') {
    event.preventDefault()
    saveRename(workflowId)
  } else if (event.key === 'Escape') {
    event.preventDefault()
    cancelRename()
  }
}

const selectWorkflow = (workflow: any, event?: MouseEvent) => {
  // Prevent event bubbling
  event?.stopPropagation()
  selectedWorkflowId.value = selectedWorkflowId.value === workflow.id ? null : workflow.id
}

const handleDoubleClick = (workflow: any, event: MouseEvent) => {
  event.stopPropagation()
  router.push(`/workflow/${workflow.id}`)
}

const handleDelete = async (workflow: any, event: MouseEvent) => {
  event.stopPropagation()
  event.preventDefault()
  await deleteWorkflow(workflow.id, `Are you sure you want to delete "${workflow.name}"?`)
  await loadWorkflows()
}

const handleToggleTrigger = async (workflow: any, value: boolean) => {
  if (value) {
    await activateTrigger(workflow.id)
  } else {
    await deactivateTrigger(workflow.id)
  }
}

const hasTrigger = (workflow: any) => {
  return workflow.nodes?.some(isNodeATrigger)
}
</script>

<template>
  <div class="workflow-list">
    <div class="page-header">
      <h1>Workflows</h1>
      <div class="header-actions">
        <ElInput
          v-model="searchQuery"
          placeholder="Search workflows by name or description..."
          :prefix-icon="Search"
          clearable
          class="search-input"
          @input="setSearchQuery"
        />
        <ElButton type="primary" :icon="Plus" @click="createWorkflow">New Workflow</ElButton>
      </div>
    </div>

    <!-- Search results info -->
    <div v-if="searchQuery" class="search-info">
      <span>Found {{ displayWorkflows.length }} workflow(s) matching "{{ searchQuery }}"</span>
      <ElButton link @click="searchQuery = ''">Clear</ElButton>
    </div>

    <div v-if="displayWorkflows.length === 0 && !isLoading" class="empty-state">
      <ElEmpty
        :description="searchQuery ? 'No workflows found matching your search' : 'No workflows yet'"
      >
        <ElButton v-if="!searchQuery" type="primary" @click="createWorkflow"
          >Create your first workflow</ElButton
        >
        <ElButton v-else @click="searchQuery = ''">Clear search</ElButton>
      </ElEmpty>
    </div>

    <!-- Help text at the top -->
    <div v-if="displayWorkflows.length > 0" class="help-text">
      <span
        >💡 Tip: Click to select • Double-click to open • Ctrl+C/V to copy/paste • Click ✏️ to
        rename</span
      >
    </div>

    <ElRow v-if="displayWorkflows.length > 0" :gutter="20" class="workflow-grid">
      <ElCol v-for="workflow in displayWorkflows" :key="workflow.id" :span="8">
        <ElCard
          :class="['workflow-card', { selected: selectedWorkflowId === workflow.id }]"
          :shadow="selectedWorkflowId === workflow.id ? 'always' : 'hover'"
          @click="selectWorkflow(workflow, $event)"
          @dblclick="handleDoubleClick(workflow, $event)"
        >
          <div class="card-content">
            <div class="card-header">
              <div v-if="editingWorkflowId === workflow.id" class="rename-input-wrapper">
                <ElInput
                  :id="`rename-input-${workflow.id}`"
                  v-model="editingName"
                  size="small"
                  @blur="saveRename(workflow.id)"
                  @keydown="handleRenameKeydown($event as KeyboardEvent, workflow.id)"
                  @click.stop
                />
              </div>
              <div v-else class="workflow-title-wrapper">
                <h3 class="workflow-name">
                  {{ workflow.name }}
                </h3>
                <ElTooltip content="Rename workflow">
                  <ElButton
                    :icon="EditPen"
                    link
                    type="primary"
                    size="small"
                    class="edit-icon"
                    @click="startRename(workflow, $event)"
                  />
                </ElTooltip>
              </div>
              <div class="card-actions">
                <ElTooltip
                  v-if="hasTrigger(workflow)"
                  :content="
                    getTriggerStatus(workflow.id).active ? 'Pause trigger' : 'Start trigger'
                  "
                >
                  <ElButton
                    :icon="getTriggerStatus(workflow.id).active ? VideoPause : VideoPlay"
                    circle
                    plain
                    :type="getTriggerStatus(workflow.id).active ? 'success' : 'warning'"
                    size="small"
                    class="trigger-btn"
                    @click.stop="
                      handleToggleTrigger(workflow, !getTriggerStatus(workflow.id).active)
                    "
                  />
                </ElTooltip>
                <ElTooltip content="Delete workflow">
                  <ElButton
                    :icon="Close"
                    circle
                    plain
                    type="danger"
                    size="small"
                    class="delete-btn"
                    @click="handleDelete(workflow, $event)"
                  />
                </ElTooltip>
              </div>
            </div>

            <div class="metadata">
              <span>{{ workflow.nodeCount }} nodes</span>
              <span class="dot">•</span>
              <span
                >Updated
                {{ new Date(workflow.updated_at || workflow.updatedAt).toLocaleDateString() }}</span
              >
            </div>
          </div>
        </ElCard>
      </ElCol>
    </ElRow>

    <ElDialog v-model="dialogVisible" title="Create New Workflow" width="500px">
      <ElForm label-width="100px">
        <ElFormItem label="Name" required>
          <ElInput
            v-model="newWorkflowName"
            placeholder="Enter workflow name"
            @keyup.enter="saveWorkflow"
          />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="dialogVisible = false">Cancel</ElButton>
        <ElButton type="primary" @click="saveWorkflow">Create</ElButton>
      </template>
    </ElDialog>
  </div>
</template>

<style scoped>
.workflow-list {
  padding: 20px;
  height: 100%;
  overflow-y: auto;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 30px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.search-input {
  width: 300px;
}

.search-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background-color: #f0f2f5;
  border-radius: 4px;
  margin-bottom: 20px;
  font-size: 14px;
  color: #606266;
}

.page-header h1 {
  margin: 0;
  font-size: 28px;
  font-weight: 600;
}

.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 60vh;
}

.workflow-grid {
  margin-top: 20px;
}

.workflow-card {
  margin-bottom: 20px;
  transition: all 0.2s;
  cursor: pointer;
  border: 1px solid #dcdfe6;
  background-color: #ffffff;
}

.workflow-card:hover {
  transform: translateY(-2px);
  border-color: #c0c4cc;
}

.workflow-card.selected {
  border: 2px solid #409eff;
  background-color: #f0f9ff;
}

.card-content {
  padding: 8px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 16px;
}

.workflow-title-wrapper {
  display: flex;
  align-items: center;
  flex: 1;
  gap: 8px;
}

.workflow-name {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #303133;
  transition: color 0.2s;
}

.edit-icon {
  opacity: 0;
  transition: opacity 0.2s;
}

.workflow-card:hover .edit-icon {
  opacity: 1;
}

.edit-icon:hover {
  background-color: #f0f9ff;
}

.rename-input-wrapper {
  flex: 1;
  margin-right: 12px;
}

.rename-input-wrapper :deep(.el-input__wrapper) {
  background-color: #ffffff;
  box-shadow: 0 0 0 1px #409eff inset;
}

.rename-input-wrapper :deep(.el-input__inner) {
  font-size: 18px;
  font-weight: 600;
}

.card-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.trigger-btn,
.delete-btn {
  width: 32px !important;
  height: 32px !important;
  padding: 0 !important;
}

.trigger-btn :deep(.el-icon) {
  font-size: 18px !important;
}

.delete-btn :deep(.el-icon) {
  font-size: 16px !important;
}

.metadata {
  display: flex;
  align-items: center;
  font-size: 12px;
  color: #909399;
  gap: 8px;
}

.metadata .dot {
  color: #dcdfe6;
}

.help-text {
  margin-bottom: 20px;
  padding: 10px 16px;
  background: linear-gradient(135deg, #f0f9ff 0%, #ecf5ff 100%);
  border: 1px solid #d9ecff;
  border-radius: 6px;
  text-align: center;
  font-size: 13px;
  color: #409eff;
  animation: fadeIn 0.3s ease-in;
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
</style>
