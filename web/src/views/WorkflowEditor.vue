<script setup lang="ts">
import { ElMessage, ElButton, ElTooltip } from 'element-plus'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Expand, Fold, Check, ArrowLeft, Document, FolderOpened } from '@element-plus/icons-vue'
import { RotateCcw } from 'lucide-vue-next'
import Editor from '../components/workflow-editor/Editor.vue'
import TriggerToggle from '../components/workflow-editor/TriggerToggle.vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import ExecutionHistoryPanel from '../components/ExecutionHistoryPanel.vue'
import VariablePanel from '../components/workflow-editor/VariablePanel.vue'
import { useWorkflowImportExport } from '../composables/persistence/useWorkflowImportExport'
import { useWorkflowPersistence } from '../composables/persistence/useWorkflowPersistence'
import { useKeyboardShortcuts } from '../composables/shared/useKeyboardShortcuts'
import { useUnsavedChanges } from '../composables/shared/useUnsavedChanges'
import { useWorkflowStore } from '../stores/workflowStore'
import { useExecutionStore } from '../stores/executionStore'
import { useWorkflowTrigger } from '../composables/trigger/useWorkflowTrigger'
import { VALIDATION_MESSAGES, SUCCESS_MESSAGES } from '@/constants'

const route = useRoute()
const router = useRouter()
const workflowStore = useWorkflowStore()
const executionStore = useExecutionStore()

// Variable panel state
const showVariablePanel = ref(true)
const selectedNodeId = computed(() => executionStore.selectedNodeId)

// Workflow trigger management
const workflowIdRef = computed(() => workflowStore.currentWorkflowId)
const nodesRef = computed(() => workflowStore.nodes)
const {
  isActive: isTriggerActive,
  isLoading: isTriggerLoading,
  hasTriggers,
  statusText,
  loadTriggerStatus,
  toggleActivation,
} = useWorkflowTrigger(workflowIdRef, nodesRef)

// Execution history panel state (default: hidden)
const showHistoryPanel = ref(false)

const toggleHistoryPanel = () => {
  showHistoryPanel.value = !showHistoryPanel.value
}

const { currentWorkflowMeta, isSaving, loadWorkflow, saveWorkflow } = useWorkflowPersistence()

const { exportWorkflow, importWorkflow } = useWorkflowImportExport({
  onImportSuccess: (data) => {
    if (data.name) {
      workflowStore.setWorkflowMetadata(workflowStore.currentWorkflowId, data.name)
    }
    unsavedChanges.markAsDirty()
  },
})

const unsavedChanges = useUnsavedChanges()
const workflowName = computed(() => currentWorkflowMeta.value.name || 'Untitled Workflow')

// Workflow name editing
const handleWorkflowNameUpdate = (newName: string) => {
  const trimmedName = newName.trim()
  if (!trimmedName) return

  // Only update if name has actually changed
  if (trimmedName === workflowStore.currentWorkflowName) {
    return
  }

  workflowStore.setWorkflowMetadata(workflowStore.currentWorkflowId, trimmedName)
  unsavedChanges.markAsDirty()
}
const handleSave = async () => {
  // With immediate creation, currentWorkflowId is ALWAYS set
  if (!workflowStore.currentWorkflowName?.trim()) {
    ElMessage.error(VALIDATION_MESSAGES.REQUIRED_PROVIDE('workflow name'))
    return
  }

  const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
    meta: { name: workflowStore.currentWorkflowName },
    showMessage: true,
  })

  if (result.success) {
    unsavedChanges.markAsSaved()
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

const handleClearExecution = () => {
  executionStore.clearExecution()
  ElMessage.success(SUCCESS_MESSAGES.EXECUTION_CLEARED)
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

  if (!workflowId) {
    // This should never happen with immediate creation
    console.warn('No workflow ID provided, redirecting to workflows list')
    router.push('/workflows')
    return
  }

  const result = await loadWorkflow(workflowId)
  if (result.success) {
    unsavedChanges.markAsSaved()
    // Load trigger status after workflow is loaded
    await loadTriggerStatus()
  } else {
    router.push('/workflows')
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
  workflowStore.resetWorkflow()
  unsavedChanges.markAsSaved()
})
</script>

<template>
  <PageLayout class="workflow-editor-page" variant="fullheight" no-padding>
    <HeaderBar
      class="workflow-header"
      :title="workflowName || 'Workflow Editor'"
      editable
      :model-value="workflowStore.currentWorkflowName"
      @update:model-value="handleWorkflowNameUpdate"
    >
      <template #left-actions>
        <!-- Back button -->
        <el-tooltip content="Go back to workflow list" placement="bottom">
          <el-button :icon="ArrowLeft" circle @click="goBack" />
        </el-tooltip>

        <!-- History panel toggle -->
        <el-tooltip
          :content="showHistoryPanel ? 'Hide Execution History' : 'Show Execution History'"
          placement="bottom"
        >
          <el-button
            :icon="showHistoryPanel ? Fold : Expand"
            circle
            @click="toggleHistoryPanel"
            :type="showHistoryPanel ? 'primary' : 'default'"
          />
        </el-tooltip>

        <!-- Clear execution state button -->
        <el-tooltip
          v-if="executionStore.hasResults"
          content="Clear execution state"
          placement="bottom"
        >
          <el-button circle @click="handleClearExecution">
            <template #icon>
              <RotateCcw :size="16" />
            </template>
          </el-button>
        </el-tooltip>

        <!-- Trigger activation toggle -->
        <TriggerToggle
          v-if="hasTriggers && workflowStore.currentWorkflowId"
          :is-active="isTriggerActive"
          :is-loading="isTriggerLoading"
          :status-text="statusText"
          @toggle="toggleActivation"
        />

        <!-- Save/Saved status -->
        <ElTooltip
          v-if="unsavedChanges.hasChanges.value"
          content="Save workflow (Ctrl+S)"
          placement="bottom"
        >
          <ElButton type="primary" @click="handleSave" :loading="isSaving"> Save </ElButton>
        </ElTooltip>
        <ElButton v-else type="success" :icon="Check" disabled>Saved</ElButton>
      </template>

      <template #actions>
        <div class="editor-actions">
          <ElTooltip content="Import workflow (Ctrl+O)" placement="bottom">
            <ElButton :icon="FolderOpened" @click="handleImport">Import</ElButton>
          </ElTooltip>

          <ElTooltip content="Export workflow (Ctrl+E)" placement="bottom">
            <ElButton :icon="Document" @click="handleExport">Export</ElButton>
          </ElTooltip>
        </div>
      </template>
    </HeaderBar>

    <div class="editor-container">
      <transition name="history-panel">
        <div v-if="showHistoryPanel && workflowStore.currentWorkflowId" class="left-panel">
          <ExecutionHistoryPanel :workflow-id="workflowStore.currentWorkflowId" />
        </div>
      </transition>

      <div class="main-content">
        <Editor />
      </div>

      <div
        v-if="showVariablePanel && selectedNodeId && executionStore.isEditingNode"
        class="right-panel"
      >
        <VariablePanel :node-id="selectedNodeId" />
      </div>
    </div>
  </PageLayout>
</template>

<style lang="scss" scoped>
.workflow-editor-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--rf-color-bg-page);
  position: relative;
  box-sizing: border-box;
}

.editor-container {
  --header-offset: calc(60px + var(--rf-spacing-2xl, 40px));
  flex: 1;
  overflow: hidden;
  position: relative;
  padding: var(--header-offset) var(--rf-spacing-xl, 24px) var(--rf-spacing-xl, 24px);
}

.left-panel,
.right-panel {
  position: absolute;
  top: calc(var(--header-offset) + var(--rf-spacing-md));
  bottom: var(--rf-spacing-xl);
  width: 320px;
  height: auto;
  z-index: 12;
  pointer-events: auto;
  overflow: hidden;
  border-radius: var(--rf-radius-large);
  transition:
    opacity var(--rf-transition-base),
    transform var(--rf-transition-base);
}

.left-panel {
  left: var(--rf-spacing-xl);
}

.right-panel {
  right: var(--rf-spacing-xl);
}

.main-content {
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.main-content :deep(.workflow-editor) {
  flex: 1;
}

.editor-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
}

:deep(.page-layout__header) {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 0;
  padding: 0;
  border: none;
  pointer-events: none;
}

:deep(.page-layout__header > *) {
  pointer-events: auto;
}

.workflow-editor-page :deep(.workflow-header) {
  position: absolute;
  top: var(--rf-spacing-xl);
  left: var(--rf-spacing-xl);
  right: var(--rf-spacing-xl);
  z-index: 10;
  border-radius: var(--rf-radius-large);
  border: 1px solid var(--rf-color-border-light);
  background: var(--rf-color-bg-container);
  background: color-mix(in srgb, var(--rf-color-bg-container) 92%, transparent);
  box-shadow: var(--rf-shadow-xl, 0 12px 24px rgba(0, 0, 0, 0.15));
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
}

.workflow-editor-page :deep(.workflow-header:hover) {
  box-shadow: var(--rf-shadow-xl, 0 16px 32px rgba(0, 0, 0, 0.2));
  transform: translateY(var(--rf-transform-lift-xs));
}

:global(html.dark) .workflow-editor-page :deep(.workflow-header) {
  border: 1px solid var(--rf-color-border-light);
  background: color-mix(in srgb, var(--rf-color-bg-container) 94%, transparent);
  box-shadow: var(--rf-shadow-xl, 0 16px 44px rgba(0, 0, 0, 0.45));
}

:global(html.dark) .workflow-editor-page :deep(.workflow-header:hover) {
  box-shadow: var(--rf-shadow-xl, 0 20px 52px rgba(0, 0, 0, 0.55));
}

.history-panel-enter-from,
.history-panel-leave-to {
  opacity: 0;
  transform: translateX(-20px);
}

.history-panel-enter-active,
.history-panel-leave-active {
  transition:
    opacity var(--rf-transition-base),
    transform var(--rf-transition-base);
}
</style>
