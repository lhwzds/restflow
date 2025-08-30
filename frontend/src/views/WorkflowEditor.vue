<script setup lang="ts">
import { 
  ArrowLeft, 
  Check, 
  Document, 
  FolderOpened
} from '@element-plus/icons-vue'
import {
  ElButton,
  ElDialog,
  ElForm,
  ElFormItem,
  ElInput,
  ElMessage,
  ElPageHeader,
  ElTag
} from 'element-plus'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Editor from '../components/Editor.vue'
import { useKeyboardShortcuts } from '../composables/shared/useKeyboardShortcuts'
import { useUnsavedChanges } from '../composables/shared/useUnsavedChanges'
import { useWorkflowImportExport } from '../composables/persistence/useWorkflowImportExport'
import { useWorkflowPersistence } from '../composables/persistence/useWorkflowPersistence'
import { useWorkflowStore } from '../stores/workflowStore'

const route = useRoute()
const router = useRouter()
const workflowStore = useWorkflowStore()

// Composables
const { currentWorkflowMeta, isSaving, loadWorkflow, saveWorkflow } =
  useWorkflowPersistence()

const { exportWorkflow, importWorkflow } = useWorkflowImportExport({
  onImportSuccess: (data) => {
    if (data.name) {
      workflowStore.setWorkflowMetadata(workflowStore.currentWorkflowId, data.name)
    }
    unsavedChanges.markAsDirty()
  },
})


const saveDialogVisible = ref(false)
const unsavedChanges = useUnsavedChanges()
const workflowName = computed(() => currentWorkflowMeta.value.name || 'Untitled Workflow')
const handleSave = async () => {
  // Show dialog if new workflow without name
  if (!workflowStore.currentWorkflowId && !workflowStore.currentWorkflowName?.trim()) {
    saveDialogVisible.value = true
    return
  }

  // Validate name
  if (!workflowStore.currentWorkflowName?.trim()) {
    ElMessage.error('Please provide a workflow name')
    return
  }

  // Save workflow
  const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
    meta: { name: workflowStore.currentWorkflowName },
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

useKeyboardShortcuts({
  'ctrl+s': handleSave,
  'meta+s': handleSave,
})

const goBack = () => {
  router.push('/workflows')
}

const handleExport = () => {
  exportWorkflow(workflowStore.currentWorkflowName || 'workflow')
}

const handleImport = () => {
  importWorkflow()
}



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
    workflowStore.setWorkflowMetadata(null, 'Untitled Workflow')
    unsavedChanges.markAsSaved() // Start with saved state for new workflow
  }
}

watch(
  () => route.params.id,
  (newId, oldId) => {
    if (newId !== oldId) {
      if (!oldId && newId === workflowStore.currentWorkflowId) {
        // From new workflow to saved workflow after save
        return
      }
      initializeWorkflow()
    }
  },
)

onMounted(() => {
  initializeWorkflow()
})

onUnmounted(() => {
  workflowStore.clearCanvas()
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
            v-model="workflowStore.currentWorkflowName"
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

<style lang="scss" scoped>
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
