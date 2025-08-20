<script setup lang="ts">
import { ArrowLeft, Check, Document, FolderOpened } from '@element-plus/icons-vue'
import {
  ElButton,
  ElDialog,
  ElForm,
  ElFormItem,
  ElInput,
  ElPageHeader,
  ElTag,
} from 'element-plus'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Editor from '../components/Editor.vue'
import { useKeyboardShortcuts } from '../composables/shared/useKeyboardShortcuts'
import { useUnsavedChanges } from '../composables/shared/useUnsavedChanges'
import { useWorkflowImportExport } from '../composables/workflow/useWorkflowImportExport'
import { useWorkflowPersistence } from '../composables/workflow/useWorkflowPersistence'
import { useWorkflowStore } from '../stores/workflowStore'

const route = useRoute()
const router = useRouter()
const workflowStore = useWorkflowStore()

// Composables
const {
  currentWorkflowId,
  currentWorkflowMeta,
  isLoading,
  isSaving,
  loadWorkflow,
  saveWorkflow,
  quickSave,
} = useWorkflowPersistence()

const { exportWorkflow, importWorkflow } = useWorkflowImportExport({
  onImportSuccess: (data) => {
    if (data.name) {
      currentWorkflowMeta.value.name = data.name
      currentWorkflowMeta.value.description = data.description || ''
    }
    unsavedChanges.markAsDirty()
  },
})

const unsavedChanges = useUnsavedChanges({
  watchSource: [() => workflowStore.nodes, () => workflowStore.edges],
})

// Local state
const saveDialogVisible = ref(false)

// Computed properties
const workflowName = computed(() => currentWorkflowMeta.value.name || 'Untitled Workflow')
const workflowDescription = computed(() => currentWorkflowMeta.value.description || '')

// Save workflow
const handleSave = () => {
  if (!currentWorkflowId.value) {
    // New workflow - show dialog
    saveDialogVisible.value = true
  } else {
    // Existing workflow - save directly
    performSave()
  }
}

// Keyboard shortcuts
useKeyboardShortcuts({
  'ctrl+s': handleSave,
  'meta+s': handleSave,
})

const performSave = async () => {
  const meta = {
    name: currentWorkflowMeta.value.name,
    description: currentWorkflowMeta.value.description,
  }

  if (!meta.name?.trim()) {
    return
  }

  const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
    meta,
    showMessage: true,
  })

  if (result.success) {
    unsavedChanges.markAsSaved()
    saveDialogVisible.value = false

    // Update URL for new workflows
    if (!route.params.id && result.id) {
      router.replace(`/workflow/${result.id}`)
    }
  }
}

// Navigation
const goBack = () => {
  if (unsavedChanges.isDirty.value) {
    if (confirm('You have unsaved changes. Are you sure you want to leave?')) {
      router.push('/workflows')
    }
  } else {
    router.push('/workflows')
  }
}

// Export/Import handlers
const handleExport = () => {
  exportWorkflow(
    currentWorkflowMeta.value.name || 'workflow',
    currentWorkflowMeta.value.description
  )
}

const handleImport = () => {
  importWorkflow()
}

// Initialization
onMounted(async () => {
  if (route.params.id) {
    const result = await loadWorkflow(route.params.id as string)
    if (result.success) {
      unsavedChanges.markAsSaved()
    } else {
      router.push('/workflows')
    }
  } else {
    // New workflow
    workflowStore.clearCanvas()
    currentWorkflowMeta.value = {
      name: 'Untitled Workflow',
      description: '',
    }
    unsavedChanges.markAsDirty()
  }
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
          <ElTag v-if="unsavedChanges.isDirty.value" type="warning" size="small">Unsaved</ElTag>
        </div>
      </template>
      <template #extra>
        <div class="header-actions">
          <ElButton v-if="!unsavedChanges.isDirty.value" type="success" :icon="Check" disabled>Saved</ElButton>
          <ElButton v-else type="primary" @click="handleSave" :loading="isSaving">Save (Ctrl+S)</ElButton>
          <ElButton :icon="FolderOpened" @click="handleImport">Import</ElButton>
          <ElButton :icon="Document" @click="handleExport">Export</ElButton>
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
            v-model="currentWorkflowMeta.name"
            placeholder="Enter workflow name"
            @keyup.enter="performSave"
          />
        </ElFormItem>
        <ElFormItem label="Description">
          <ElInput
            v-model="currentWorkflowMeta.description"
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
