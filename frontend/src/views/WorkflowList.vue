<script setup lang="ts">
import { Delete, DocumentCopy, Edit, EditPen, Plus, Search } from '@element-plus/icons-vue'
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
  ElTag,
  ElTooltip,
} from 'element-plus'
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useWorkflowList } from '../composables/workflow/useWorkflowList'

const router = useRouter()

// Use composable for workflow list management
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

// Local dialog state
const dialogVisible = ref(false)
const isEditing = ref(false)
const currentWorkflow = ref<{ id?: string; name: string; description?: string }>({
  name: '',
  description: '',
})

// Initialize on mount
onMounted(async () => {
  await loadWorkflows()
})

// Computed properties for display
const displayWorkflows = computed(() => {
  return filteredWorkflows.value.map(w => ({
    ...w,
    createdAt: w.created_at || new Date().toISOString(),
    updatedAt: w.updated_at || new Date().toISOString(),
    nodeCount: 0, // Backend doesn't return node count yet
    status: 'draft' as const,
  }))
})

// Dialog handlers
const createWorkflow = () => {
  isEditing.value = false
  currentWorkflow.value = {
    name: '',
    description: '',
  }
  dialogVisible.value = true
}

const editWorkflowName = (workflow: any) => {
  isEditing.value = true
  currentWorkflow.value = {
    id: workflow.id,
    name: workflow.name,
    description: workflow.description,
  }
  dialogVisible.value = true
}

const saveWorkflow = async () => {
  if (!currentWorkflow.value.name?.trim()) {
    ElMessage.error('Please enter a workflow name')
    return
  }

  if (isEditing.value && currentWorkflow.value.id) {
    // Rename workflow
    const result = await renameWorkflow(currentWorkflow.value.id, currentWorkflow.value.name)
    if (result.success) {
      dialogVisible.value = false
    }
  } else {
    // Create new workflow and navigate to editor
    router.push(`/workflow?name=${encodeURIComponent(currentWorkflow.value.name)}&description=${encodeURIComponent(currentWorkflow.value.description || '')}`)
  }
}

const openWorkflow = (workflow: any) => {
  router.push(`/workflow/${workflow.id}`)
}

const handleDuplicate = async (workflow: any) => {
  await duplicateWorkflow(workflow.id, `${workflow.name} (Copy)`)
}

const handleDelete = async (workflow: any) => {
  await deleteWorkflow(workflow.id, `Are you sure you want to delete workflow "${workflow.name}"?`)
  await loadWorkflows()
}

const getStatusColor = (status: string) => {
  switch (status) {
    case 'published':
      return 'success'
    case 'archived':
      return 'info'
    default:
      return 'warning'
  }
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
      <ElEmpty :description="searchQuery ? 'No workflows found matching your search' : 'No workflows yet'">
        <ElButton v-if="!searchQuery" type="primary" @click="createWorkflow">Create your first workflow</ElButton>
        <ElButton v-else @click="searchQuery = ''">Clear search</ElButton>
      </ElEmpty>
    </div>

    <ElRow v-else :gutter="20" class="workflow-grid">
      <ElCol v-for="workflow in displayWorkflows" :key="workflow.id" :span="8">
        <ElCard class="workflow-card" shadow="hover">
          <template #header>
            <div class="card-header">
              <div class="workflow-title">
                <h3>{{ workflow.name }}</h3>
                <ElTag :type="getStatusColor(workflow.status)" size="small">
                  {{ workflow.status }}
                </ElTag>
              </div>
              <div class="card-actions">
                <ElTooltip content="Open Editor">
                  <ElButton :icon="EditPen" circle size="small" @click="openWorkflow(workflow)" />
                </ElTooltip>
                <ElTooltip content="Rename">
                  <ElButton :icon="Edit" circle size="small" @click="editWorkflowName(workflow)" />
                </ElTooltip>
                <ElTooltip content="Duplicate">
                  <ElButton
                    :icon="DocumentCopy"
                    circle
                    size="small"
                    @click="handleDuplicate(workflow)"
                  />
                </ElTooltip>
                <ElTooltip content="Delete">
                  <ElButton
                    :icon="Delete"
                    circle
                    size="small"
                    type="danger"
                    @click="handleDelete(workflow)"
                  />
                </ElTooltip>
              </div>
            </div>
          </template>
          <div class="workflow-info">
            <p class="description">{{ workflow.description || 'No description' }}</p>
            <div class="metadata">
              <span>{{ workflow.nodeCount }} nodes</span>
              <span>Updated {{ new Date(workflow.updated_at || workflow.updatedAt).toLocaleDateString() }}</span>
            </div>
          </div>
        </ElCard>
      </ElCol>
    </ElRow>

    <ElDialog
      v-model="dialogVisible"
      :title="isEditing ? 'Edit Workflow' : 'Create New Workflow'"
      width="500px"
    >
      <ElForm :model="currentWorkflow" label-width="100px">
        <ElFormItem label="Name" required>
          <ElInput v-model="currentWorkflow.name" placeholder="Enter workflow name" />
        </ElFormItem>
        <ElFormItem label="Description">
          <ElInput
            v-model="currentWorkflow.description"
            type="textarea"
            :rows="3"
            placeholder="Enter workflow description (optional)"
          />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="dialogVisible = false">Cancel</ElButton>
        <ElButton type="primary" @click="saveWorkflow">
          {{ isEditing ? 'Update' : 'Create' }}
        </ElButton>
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
  transition: transform 0.2s;
}

.workflow-card:hover {
  transform: translateY(-2px);
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.workflow-title {
  flex: 1;
}

.workflow-title h3 {
  margin: 0 0 8px 0;
  font-size: 18px;
  font-weight: 600;
}

.card-actions {
  display: flex;
  gap: 8px;
}

.workflow-info {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.description {
  color: #666;
  margin: 0;
  font-size: 14px;
  line-height: 1.5;
}

.metadata {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: #999;
}
</style>
