<script setup lang="ts">
import { Delete, DocumentCopy, Edit, EditPen, Plus } from '@element-plus/icons-vue'
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
  ElMessageBox,
  ElRow,
  ElTag,
  ElTooltip,
} from 'element-plus'
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { workflowService } from '../services/workflowService'

interface Workflow {
  id: string
  name: string
  description?: string
  createdAt: string
  updatedAt: string
  nodeCount: number
  status: 'draft' | 'published' | 'archived'
}

const router = useRouter()
const workflows = ref<Workflow[]>([])
const loading = ref(false)
const dialogVisible = ref(false)
const isEditing = ref(false)
const currentWorkflow = ref<Partial<Workflow>>({
  name: '',
  description: '',
})

onMounted(() => {
  loadWorkflows()
})

const loadWorkflows = async () => {
  loading.value = true
  try {
    const response = await workflowService.list()
    if (response.status === 'success' && response.data) {
      workflows.value = response.data.map((w: any) => ({
        id: w.id,
        name: w.name,
        description: w.description,
        createdAt: w.created_at || new Date().toISOString(),
        updatedAt: w.updated_at || new Date().toISOString(),
        nodeCount: w.nodes?.length || 0,
        status: 'draft',
      }))
    } else {
      workflows.value = []
    }
  } catch (error) {
    console.error('Failed to load workflows:', error)
    ElMessage.error('Failed to load workflows from server')
    workflows.value = []
  } finally {
    loading.value = false
  }
}

// saveWorkflows removed - using backend API instead

const createWorkflow = () => {
  isEditing.value = false
  currentWorkflow.value = {
    name: '',
    description: '',
  }
  dialogVisible.value = true
}

const editWorkflow = (workflow: Workflow) => {
  isEditing.value = true
  currentWorkflow.value = { ...workflow }
  dialogVisible.value = true
}

const saveWorkflow = async () => {
  if (!currentWorkflow.value.name) {
    ElMessage.error('Please enter a workflow name')
    return
  }

  try {
    if (isEditing.value && currentWorkflow.value.id) {
      // Update existing workflow
      await workflowService.update(
        currentWorkflow.value.id,
        [], // Empty nodes for now
        [], // Empty edges for now
        {
          name: currentWorkflow.value.name,
          description: currentWorkflow.value.description,
        }
      )
      ElMessage.success('Workflow updated successfully')
    } else {
      // Create new workflow
      await workflowService.createFromVueFlow(
        [], // Empty nodes for now
        [], // Empty edges for now
        {
          name: currentWorkflow.value.name!,
          description: currentWorkflow.value.description,
        }
      )
      ElMessage.success('Workflow created successfully')
    }
    
    dialogVisible.value = false
    await loadWorkflows() // Reload the list
  } catch (error) {
    console.error('Failed to save workflow:', error)
    ElMessage.error(isEditing.value ? 'Failed to update workflow' : 'Failed to create workflow')
  }
}

const openWorkflow = (workflow: Workflow) => {
  router.push(`/workflow/${workflow.id}`)
}

const duplicateWorkflow = async (workflow: Workflow) => {
  try {
    // Get the workflow data from backend
    const originalData = await workflowService.get(workflow.id)
    
    // Create a copy with new name using raw backend format
    const duplicateData = {
      ...originalData,
      id: `workflow-${Date.now()}`,
      name: `${workflow.name} (Copy)`,
      description: workflow.description,
    }
    
    await workflowService.create(duplicateData)
    
    ElMessage.success('Workflow duplicated successfully')
    await loadWorkflows() // Reload the list
  } catch (error) {
    console.error('Failed to duplicate workflow:', error)
    ElMessage.error('Failed to duplicate workflow')
  }
}

const deleteWorkflow = async (workflow: Workflow) => {
  try {
    await ElMessageBox.confirm(
      `Are you sure you want to delete workflow "${workflow.name}"?`,
      'Delete Workflow',
      {
        confirmButtonText: 'Delete',
        cancelButtonText: 'Cancel',
        type: 'warning',
      },
    )

    await workflowService.delete(workflow.id)
    ElMessage.success('Workflow deleted successfully')
    await loadWorkflows() // Reload the list
  } catch (error: any) {
    if (error !== 'cancel') {
      console.error('Failed to delete workflow:', error)
      ElMessage.error('Failed to delete workflow')
    }
    // User cancelled - do nothing
  }
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
      <ElButton type="primary" :icon="Plus" @click="createWorkflow">New Workflow</ElButton>
    </div>

    <div v-if="workflows.length === 0" class="empty-state">
      <ElEmpty description="No workflows yet">
        <ElButton type="primary" @click="createWorkflow">Create your first workflow</ElButton>
      </ElEmpty>
    </div>

    <ElRow v-else :gutter="20" class="workflow-grid">
      <ElCol v-for="workflow in workflows" :key="workflow.id" :span="8">
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
                  <ElButton :icon="Edit" circle size="small" @click="editWorkflow(workflow)" />
                </ElTooltip>
                <ElTooltip content="Duplicate">
                  <ElButton
                    :icon="DocumentCopy"
                    circle
                    size="small"
                    @click="duplicateWorkflow(workflow)"
                  />
                </ElTooltip>
                <ElTooltip content="Delete">
                  <ElButton
                    :icon="Delete"
                    circle
                    size="small"
                    type="danger"
                    @click="deleteWorkflow(workflow)"
                  />
                </ElTooltip>
              </div>
            </div>
          </template>
          <div class="workflow-info">
            <p class="description">{{ workflow.description || 'No description' }}</p>
            <div class="metadata">
              <span>{{ workflow.nodeCount }} nodes</span>
              <span>Updated {{ new Date(workflow.updatedAt).toLocaleDateString() }}</span>
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
