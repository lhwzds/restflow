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
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
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
const { currentWorkflowId, currentWorkflowMeta, isSaving, loadWorkflow, saveWorkflow } =
  useWorkflowPersistence()

const { exportWorkflow, importWorkflow } = useWorkflowImportExport({
  onImportSuccess: (data) => {
    if (data.name) {
      currentWorkflowMeta.value.name = data.name
    }
    unsavedChanges.markAsDirty()
  },
})

// Local state
const saveDialogVisible = ref(false)

// Use unsaved changes composable
const unsavedChanges = useUnsavedChanges()

// Computed properties
const workflowName = computed(() => currentWorkflowMeta.value.name || 'Untitled Workflow')

// Save workflow (combined logic)
const handleSave = async () => {
  // Show dialog if new workflow without name
  if (!currentWorkflowId.value && !currentWorkflowMeta.value.name?.trim()) {
    saveDialogVisible.value = true
    return
  }

  // Validate name
  if (!currentWorkflowMeta.value.name?.trim()) {
    ElMessage.error('Please provide a workflow name')
    return
  }

  // Save workflow
  const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
    meta: { name: currentWorkflowMeta.value.name },
    showMessage: true,
  })

  if (result.success) {
    unsavedChanges.markAsSaved()
    saveDialogVisible.value = false

    // Update URL for new workflows
    if (!route.params.id && result.id) {
      currentWorkflowId.value = result.id
      router.replace(`/workflow/${result.id}`)
    }
  }
}

// Keyboard shortcuts
useKeyboardShortcuts({
  'ctrl+s': handleSave,
  'meta+s': handleSave,
})

// Navigation
const goBack = () => {
  // Navigation guard in useUnsavedChanges will handle confirmation
  router.push('/workflows')
}

// Export/Import handlers
const handleExport = () => {
  exportWorkflow(currentWorkflowMeta.value.name || 'workflow')
}

const handleImport = () => {
  importWorkflow()
}

// Initialize workflow based on route
const initializeWorkflow = async () => {
  const workflowId = route.params.id as string

  if (workflowId) {
    const result = await loadWorkflow(workflowId)
    if (result.success) {
      unsavedChanges.markAsSaved()
    } else {
      router.push('/workflows')
    }
  } else {
    workflowStore.clearCanvas()
    currentWorkflowMeta.value = {
      name: 'Untitled Workflow',
    }
    currentWorkflowId.value = null
    unsavedChanges.markAsSaved() // Start with saved state for new workflow
  }
}

// Watch for route changes to reinitialize
watch(
  () => route.params.id,
  (newId, oldId) => {
    if (newId !== oldId) {
      if (!oldId && newId === currentWorkflowId.value) {
        // From new workflow to saved workflow after save
        return
      }
      initializeWorkflow()
    }
  },
)

// Initial mount
onMounted(() => {
  initializeWorkflow()
})

// Clean up on unmount
onUnmounted(() => {
  // Clear everything when leaving workflow editor
  workflowStore.clearCanvas()
  currentWorkflowId.value = null
  currentWorkflowMeta.value = {}
  unsavedChanges.markAsSaved()
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
          <ElTag v-if="unsavedChanges.hasChanges.value" type="warning" size="small">Unsaved</ElTag>
        </div>
      </template>
      <template #extra>
        <div class="header-actions">
          <ElButton v-if="!unsavedChanges.hasChanges.value" type="success" :icon="Check" disabled
            >Saved</ElButton
          >
          <ElButton v-else type="primary" @click="handleSave" :loading="isSaving"
            >Save (Ctrl+S)</ElButton
          >
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
            @keyup.enter="handleSave"
          />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="saveDialogVisible = false">Cancel</ElButton>
        <ElButton type="primary" @click="handleSave">Save</ElButton>
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
