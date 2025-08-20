<script setup lang="ts">
import { ArrowLeft, Check, Document, FolderOpened } from '@element-plus/icons-vue'
import { ElButton, ElDialog, ElForm, ElFormItem, ElInput, ElMessage, ElPageHeader, ElTag } from 'element-plus'
import { computed, onMounted, ref, watch } from 'vue'
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

// Initialize without marking as unsaved
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

// Handle VueFlow ready event - set up change tracking after VueFlow initializes
const onFlowReady = () => {
  // Set up change detection only after VueFlow is ready
  watch(
    [() => workflowStore.nodes, () => workflowStore.edges],
    () => unsavedChanges.markAsDirty(),
    { deep: true }
  )
}

// Initialization
onMounted(async () => {
  if (route.params.id) {
    // Loading existing workflow
    const result = await loadWorkflow(route.params.id as string)
    if (result.success) {
      // Mark as saved after load
      unsavedChanges.markAsSaved()
    } else {
      router.push('/workflows')
    }
  } else {
    // New workflow
    workflowStore.clearCanvas()
    currentWorkflowMeta.value = {
      name: 'Untitled Workflow',
    }
    
    // Mark new workflows as unsaved
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
      <Editor @ready="onFlowReady" />
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
