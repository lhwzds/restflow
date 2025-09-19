<script setup lang="ts">
import { ElDialog, ElForm, ElFormItem, ElInput, ElMessage } from 'element-plus'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Editor from '../components/workflow-editor/Editor.vue'
import EditorHeader from '../components/workflow-editor/EditorHeader.vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import { useWorkflowImportExport } from '../composables/persistence/useWorkflowImportExport'
import { useWorkflowPersistence } from '../composables/persistence/useWorkflowPersistence'
import { useKeyboardShortcuts } from '../composables/shared/useKeyboardShortcuts'
import { useUnsavedChanges } from '../composables/shared/useUnsavedChanges'
import { useWorkflowStore } from '../stores/workflowStore'

const route = useRoute()
const router = useRouter()
const workflowStore = useWorkflowStore()

const { currentWorkflowMeta, isSaving, loadWorkflow, saveWorkflow } = useWorkflowPersistence()

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
  if (!workflowStore.currentWorkflowId && !workflowStore.currentWorkflowName?.trim()) {
    saveDialogVisible.value = true
    return
  }

  if (!workflowStore.currentWorkflowName?.trim()) {
    ElMessage.error('Please provide a workflow name')
    return
  }

  const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
    meta: { name: workflowStore.currentWorkflowName },
    showMessage: true,
  })

  if (result.success) {
    unsavedChanges.markAsSaved()
    saveDialogVisible.value = false

    // Navigate to saved workflow URL after initial save
    if (!route.params.id && result.id) {
      router.replace(`/workflow/${result.id}`)
    }
  }
}

const goBack = () => {
  router.push('/workflows')
}

const handleExport = () => {
  exportWorkflow(workflowStore.currentWorkflowName || 'workflow')
}

const handleImport = () => {
  importWorkflow()
}

// Register keyboard shortcuts for common operations
useKeyboardShortcuts({
  'ctrl+s': handleSave,
  'meta+s': handleSave,
  'ctrl+o': handleImport,
  'meta+o': handleImport,
  'ctrl+e': handleExport,
  'meta+e': handleExport,
})

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
    unsavedChanges.markAsSaved() // New workflows start in saved state
  }
}

// Handle route changes to load different workflows
watch(
  () => route.params.id,
  (newId, oldId) => {
    if (newId !== oldId) {
      if (!oldId && newId === workflowStore.currentWorkflowId) {
        // Prevent reinitialization when URL updates after save
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
  <PageLayout variant="fullheight" no-padding>
    <HeaderBar :title="workflowName || 'Workflow Editor'">
      <template #actions>
        <EditorHeader
          :has-unsaved-changes="unsavedChanges.hasChanges.value"
          :is-saving="isSaving"
          @back="goBack"
          @save="handleSave"
          @import="handleImport"
          @export="handleExport"
        />
      </template>
    </HeaderBar>

    <div class="editor-container">
      <Editor />
    </div>

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
  </PageLayout>
</template>

<style lang="scss" scoped>
.editor-container {
  flex: 1;
  overflow: hidden;
  position: relative;
}
</style>
