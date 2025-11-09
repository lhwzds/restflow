import type { Edge, Node } from '@vue-flow/core'
import { ElMessage } from 'element-plus'
import { onUnmounted, ref, computed } from 'vue'
import * as workflowsApi from '../../api/workflows'
import type { Workflow } from '@/types/generated/Workflow'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useWorkflowConverter } from '../editor/useWorkflowConverter'
import { SUCCESS_MESSAGES, ERROR_MESSAGES, VALIDATION_MESSAGES, AUTO_SAVE_TIMING } from '@/constants'

export interface SaveOptions {
  showMessage?: boolean
  meta?: Partial<Workflow>
}

export interface LoadOptions {
  showMessage?: boolean
}

export function useWorkflowPersistence() {
  const workflowStore = useWorkflowStore()

  // State
  const isLoading = ref(false)
  const isSaving = ref(false)
  const lastSavedAt = ref<Date | null>(null)
  
  // Use workflow metadata from store
  const currentWorkflowId = computed(() => workflowStore.currentWorkflowId)
  const currentWorkflowMeta = computed(() => ({
    name: workflowStore.currentWorkflowName
  }))

  // Auto-save timer - scoped to this composable instance
  let autoSaveTimer: ReturnType<typeof setInterval> | null = null

  /**
   * Load workflow by ID with input validation
   */
  const loadWorkflow = async (id: string, options: LoadOptions = {}) => {
    if (!id || typeof id !== 'string') {
      const error = ERROR_MESSAGES.INVALID_FORMAT('workflow ID')
      ElMessage.error(error)
      return { success: false, error }
    }

    const { showMessage = true } = options

    isLoading.value = true
    try {
      const workflow = await workflowsApi.getWorkflow(id)

      if (!workflow) {
        throw new Error('Workflow not found')
      }

      // Convert and load into store
      const { convertFromBackendFormat } = useWorkflowConverter()
      const { nodes, edges } = convertFromBackendFormat(workflow)
      workflowStore.loadWorkflow(nodes, edges)

      // Update current workflow info in store
      workflowStore.setWorkflowMetadata(workflow.id, workflow.name)

      return {
        success: true,
        data: workflow,
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : ERROR_MESSAGES.FAILED_TO_LOAD('workflow')
      console.error('Failed to load workflow:', error)

      if (showMessage) {
        ElMessage.error(message)
      }

      return {
        success: false,
        error: message,
      }
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Save workflow (create or update) with validation
   */
  const saveWorkflow = async (nodes: Node[], edges: Edge[], options: SaveOptions = {}) => {
    // Input validation
    if (!Array.isArray(nodes) || !Array.isArray(edges)) {
      const error = ERROR_MESSAGES.INVALID_FORMAT('nodes or edges data')
      ElMessage.error(error)
      return { success: false, error }
    }

    const { showMessage = true, meta = {} } = options

    // Merge with current meta
    const workflowName = meta.name || workflowStore.currentWorkflowName

    if (!workflowName?.trim()) {
      ElMessage.error(VALIDATION_MESSAGES.ENTER_WORKFLOW_NAME)
      return { success: false, error: 'Name is required' }
    }

    isSaving.value = true
    try {
      // With immediate creation, workflowId is always set
      const workflowId = workflowStore.currentWorkflowId
      if (!workflowId) {
        throw new Error('Cannot save workflow without ID')
      }

      const { convertToBackendFormat } = useWorkflowConverter()
      const workflow = convertToBackendFormat(nodes, edges, {
        name: workflowName,
        id: workflowId,
      })

      // Always update existing workflow
      await workflowsApi.updateWorkflow(workflow.id, workflow)

      if (showMessage) {
        ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_UPDATED)
      }

      // Update metadata in store
      workflowStore.setWorkflowMetadata(workflowId, workflowName)
      lastSavedAt.value = new Date()

      return {
        success: true,
        data: { id: workflowId },
        id: workflowId,
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : ERROR_MESSAGES.FAILED_TO_SAVE('workflow')
      console.error('Failed to save workflow:', error)

      if (showMessage) {
        ElMessage.error(message)
      }

      return {
        success: false,
        error: message,
      }
    } finally {
      isSaving.value = false
    }
  }

  /**
   * Create a new workflow (reset current)
   */
  const createNewWorkflow = () => {
    workflowStore.resetWorkflow()
    lastSavedAt.value = null
  }

  /**
   * Quick save (save with current metadata)
   */
  const quickSave = async () => {
    return saveWorkflow(workflowStore.nodes, workflowStore.edges, {
      showMessage: true,
    })
  }

  /**
   * Save as new workflow (duplicate)
   */
  const saveAsNew = async (name: string) => {
    if (!name?.trim()) {
      ElMessage.error(VALIDATION_MESSAGES.ENTER_WORKFLOW_NAME)
      return { success: false, error: 'Name is required' }
    }

    isSaving.value = true
    try {
      // Generate new workflow ID
      const newId = `workflow-${Date.now()}-${Math.random().toString(36).substring(2, 11)}`

      // Create workflow object with current nodes/edges
      const { convertToBackendFormat } = useWorkflowConverter()
      const workflow = convertToBackendFormat(workflowStore.nodes, workflowStore.edges, {
        name,
        id: newId,
      })

      // Call create API directly
      const result = await workflowsApi.createWorkflow(workflow)

      // Update store with new workflow metadata
      workflowStore.setWorkflowMetadata(result.id, name)
      lastSavedAt.value = new Date()

      ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_CREATED)

      return {
        success: true,
        data: { id: result.id },
        id: result.id,
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : ERROR_MESSAGES.FAILED_TO_SAVE('workflow')
      console.error('Failed to save as new workflow:', error)
      ElMessage.error(message)

      return {
        success: false,
        error: message,
      }
    } finally {
      isSaving.value = false
    }
  }

  /**
   * Auto-save functionality with proper cleanup
   */
  let isAutoSaving = false

  const enableAutoSave = (intervalMs: number = AUTO_SAVE_TIMING.DEFAULT_INTERVAL) => {
    if (intervalMs < AUTO_SAVE_TIMING.MIN_INTERVAL) {
      console.warn('Auto-save interval too short, using minimum of 10 seconds')
      intervalMs = AUTO_SAVE_TIMING.MIN_INTERVAL
    }

    disableAutoSave() // Clear any existing timer

    autoSaveTimer = setInterval(async () => {
      // Prevent overlapping auto-saves
      if (workflowStore.currentWorkflowId && !isSaving.value && !isAutoSaving) {
        isAutoSaving = true
        try {
          await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
            showMessage: false,
          })
        } finally {
          isAutoSaving = false
        }
      }
    }, intervalMs)
  }

  const disableAutoSave = () => {
    if (autoSaveTimer) {
      clearInterval(autoSaveTimer)
      autoSaveTimer = null
    }
  }

  /**
   * Check if workflow exists
   */
  const checkWorkflowExists = async (id: string): Promise<boolean> => {
    if (!id || typeof id !== 'string') {
      return false
    }

    try {
      const workflow = await workflowsApi.getWorkflow(id)
      return !!workflow
    } catch {
      return false
    }
  }

  // Cleanup on unmount
  onUnmounted(() => {
    disableAutoSave()
  })

  return {
    // State
    isLoading,
    isSaving,
    lastSavedAt,
    currentWorkflowId,
    currentWorkflowMeta,

    // Methods
    loadWorkflow,
    saveWorkflow,
    createNewWorkflow,
    quickSave,
    saveAsNew,
    checkWorkflowExists,

    // Auto-save
    enableAutoSave,
    disableAutoSave,
  }
}
