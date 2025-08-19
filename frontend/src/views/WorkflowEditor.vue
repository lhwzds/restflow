<script setup lang="ts">
import { ArrowLeft, Check, Document, FolderOpened } from '@element-plus/icons-vue'
import {
  ElButton,
  ElDialog,
  ElForm,
  ElFormItem,
  ElInput,
  ElMessage,
  ElPageHeader,
  ElTag,
} from 'element-plus'
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Editor from '../components/Editor.vue'
import { convertFromBackendFormat, workflowService } from '../services/workflowService'
import { useWorkflowStore } from '../stores/workflowStore'

const route = useRoute()
const router = useRouter()
const workflowStore = useWorkflowStore()

const workflowId = ref<string>('')
const workflowName = ref<string>('Untitled Workflow')
const workflowDescription = ref<string>('')
const saveDialogVisible = ref(false)
const isSaved = ref(true)
const hasUnsavedChanges = ref(false)

// Watch for changes in nodes and edges
watch(
  [() => workflowStore.nodes, () => workflowStore.edges],
  () => {
    if (isSaved.value) {
      isSaved.value = false
      hasUnsavedChanges.value = true
    }
  },
  { deep: true },
)

const loadWorkflow = async (id: string) => {
  try {
    const workflow = await workflowService.get(id)

    if (workflow) {
      workflowName.value = workflow.name
      workflowDescription.value = workflow.description || ''

      // Convert backend format to VueFlow format
      const { nodes, edges } = convertFromBackendFormat(workflow)

      workflowStore.loadWorkflow(nodes, edges)
      isSaved.value = true
      hasUnsavedChanges.value = false
    } else {
      ElMessage.error('Workflow not found')
      router.push('/workflows')
    }
  } catch (error) {
    console.error('Failed to load workflow:', error)
    ElMessage.error('Failed to load workflow')
    router.push('/workflows')
  }
}

const saveWorkflow = () => {
  if (!workflowId.value) {
    // New workflow - show dialog
    saveDialogVisible.value = true
  } else {
    // Existing workflow - save directly
    performSave()
  }
}

const performSave = async () => {
  if (!workflowName.value.trim()) {
    ElMessage.error('Please enter a workflow name')
    return
  }

  try {
    if (!workflowId.value) {
      // Create new workflow
      const response = await workflowService.createFromVueFlow(
        workflowStore.nodes,
        workflowStore.edges,
        {
          name: workflowName.value,
          description: workflowDescription.value,
        },
      )

      // Extract ID from response
      workflowId.value = response.id || `workflow-${Date.now()}`

      // Update URL to include the new workflow ID
      router.replace(`/workflow/${workflowId.value}`)

      ElMessage.success('Workflow created successfully')
    } else {
      // Update existing workflow
      await workflowService.update(workflowId.value, workflowStore.nodes, workflowStore.edges, {
        name: workflowName.value,
        description: workflowDescription.value,
      })

      ElMessage.success('Workflow updated successfully')
    }

    isSaved.value = true
    hasUnsavedChanges.value = false
    saveDialogVisible.value = false
  } catch (error) {
    console.error('Failed to save workflow:', error)
    ElMessage.error('Failed to save workflow')
  }
}

const goBack = () => {
  if (hasUnsavedChanges.value) {
    if (confirm('You have unsaved changes. Are you sure you want to leave?')) {
      router.push('/workflows')
    }
  } else {
    router.push('/workflows')
  }
}

const exportWorkflow = () => {
  const data = {
    name: workflowName.value,
    description: workflowDescription.value,
    nodes: workflowStore.nodes,
    edges: workflowStore.edges,
    exportedAt: new Date().toISOString(),
  }

  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = `${workflowName.value.replace(/\s+/g, '-').toLowerCase()}.json`
  link.click()
  URL.revokeObjectURL(url)

  ElMessage.success('Workflow exported successfully')
}

const importWorkflow = () => {
  const input = document.createElement('input')
  input.type = 'file'
  input.accept = '.json'
  input.onchange = async (e: Event) => {
    const file = (e.target as HTMLInputElement).files?.[0]
    if (!file) return

    try {
      const text = await file.text()
      const data = JSON.parse(text)

      if (data.nodes && data.edges) {
        const { nodes, edges } = convertFromBackendFormat(data)
        workflowStore.updateWorkflow(nodes, edges)

        if (data.name) {
          workflowName.value = data.name
          workflowDescription.value = data.description || ''
        }

        hasUnsavedChanges.value = true
        isSaved.value = false
        ElMessage.success('Workflow imported successfully')
      } else {
        ElMessage.error('Invalid workflow file format')
      }
    } catch (error) {
      ElMessage.error('Failed to import workflow')
    }
  }
  input.click()
}

// Keyboard shortcuts
const handleKeyDown = (e: KeyboardEvent) => {
  // Ctrl/Cmd + S to save
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault()
    saveWorkflow()
  }
}

// Lifecycle hooks - combined initialization
onMounted(async () => {
  // Load workflow or initialize new one
  if (route.params.id) {
    workflowId.value = route.params.id as string
    await loadWorkflow(workflowId.value)
  } else {
    // New workflow
    workflowStore.clearCanvas()
    isSaved.value = false
  }

  // Add keyboard shortcuts
  document.addEventListener('keydown', handleKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeyDown)
})
</script>

<template>
  <div class="workflow-editor-page">
    <ElPageHeader @back="goBack" class="page-header">
      <template #icon>
        <ArrowLeft />
      </template>
      <template #content>
        <div class="header-content">
          <span class="workflow-name">{{ workflowName }}</span>
          <ElTag v-if="!isSaved" type="warning" size="small">Unsaved</ElTag>
        </div>
      </template>
      <template #extra>
        <div class="header-actions">
          <ElButton v-if="isSaved" type="success" :icon="Check" disabled>Saved</ElButton>
          <ElButton v-else type="primary" @click="saveWorkflow">Save (Ctrl+S)</ElButton>
          <ElButton :icon="FolderOpened" @click="importWorkflow">Import</ElButton>
          <ElButton :icon="Document" @click="exportWorkflow">Export</ElButton>
        </div>
      </template>
    </ElPageHeader>

    <div class="editor-container">
      <Editor />
    </div>

    <!-- Save Dialog for new workflows -->
    <ElDialog
      v-model="saveDialogVisible"
      title="Save Workflow"
      width="500px"
      :close-on-click-modal="false"
    >
      <ElForm label-width="100px">
        <ElFormItem label="Name" required>
          <ElInput
            v-model="workflowName"
            placeholder="Enter workflow name"
            @keyup.enter="performSave"
          />
        </ElFormItem>
        <ElFormItem label="Description">
          <ElInput
            v-model="workflowDescription"
            type="textarea"
            :rows="3"
            placeholder="Enter workflow description (optional)"
          />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="saveDialogVisible = false">Cancel</ElButton>
        <ElButton type="primary" @click="performSave">Save</ElButton>
      </template>
    </ElDialog>
  </div>
</template>

<style scoped>
.workflow-editor-page {
  height: 100vh;
  display: flex;
  flex-direction: column;
}

.page-header {
  padding: 12px 20px;
  border-bottom: 1px solid #e4e7ed;
  background: white;
  flex-shrink: 0;
}

.header-content {
  display: flex;
  align-items: center;
  gap: 12px;
}

.workflow-name {
  font-size: 18px;
  font-weight: 600;
}

.header-actions {
  display: flex;
  gap: 12px;
}

.editor-container {
  flex: 1;
  overflow: hidden;
  position: relative;
}
</style>
