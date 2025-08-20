import type { Edge, Node } from '@vue-flow/core'
import { ElMessage } from 'element-plus'
import { onUnmounted, ref } from 'vue'
import {
  convertFromBackendFormat,
  workflowService,
  type WorkflowMeta,
} from '../../services/workflowService'
import { useWorkflowStore } from '../../stores/workflowStore'

export interface SaveOptions {
  showMessage?: boolean
  meta?: Partial<WorkflowMeta>
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
  const currentWorkflowId = ref<string | null>(null)
  const currentWorkflowMeta = ref<Partial<WorkflowMeta>>({})

  // Auto-save timer - scoped to this composable instance
  let autoSaveTimer: ReturnType<typeof setInterval> | null = null

  /**
   * Load workflow by ID with input validation
   */
  const loadWorkflow = async (id: string, options: LoadOptions = {}) => {
    if (!id || typeof id !== 'string') {
      const error = 'Invalid workflow ID'
      ElMessage.error(error)
      return { success: false, error }
    }

    const { showMessage = true } = options

    isLoading.value = true
    try {
      const workflow = await workflowService.get(id)

      if (!workflow) {
        throw new Error('Workflow not found')
      }

      // Convert and load into store
      const { nodes, edges } = convertFromBackendFormat(workflow)
      workflowStore.loadWorkflow(nodes, edges)

      // Update current workflow info
      currentWorkflowId.value = workflow.id
      currentWorkflowMeta.value = {
        name: workflow.name,
        description: workflow.description,
        created_at: workflow.created_at,
        updated_at: workflow.updated_at,
      }

      if (showMessage) {
        ElMessage.success('Workflow loaded successfully')
      }

      return {
        success: true,
        data: workflow,
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to load workflow'
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
      const error = 'Invalid nodes or edges data'
      ElMessage.error(error)
      return { success: false, error }
    }

    const { showMessage = true, meta = {} } = options

    // Merge with current meta
    const workflowMeta = {
      ...currentWorkflowMeta.value,
      ...meta,
    }

    if (!workflowMeta.name?.trim()) {
      ElMessage.error('Please provide a workflow name')
      return { success: false, error: 'Name is required' }
    }

    isSaving.value = true
    try {
      let response

      if (!currentWorkflowId.value) {
        currentWorkflowId.value = `workflow-${Date.now()}-${Math.random().toString(36).substring(2, 11)}`
      }

      const workflowData = {
        ...workflowMeta,
        id: currentWorkflowId.value,
        nodes,
        edges,
      }

      response = await workflowService.save(workflowData)

      if (showMessage) {
        ElMessage.success(
          currentWorkflowId.value
            ? 'Workflow updated successfully'
            : 'Workflow created successfully',
        )
      }

      // Update metadata and timestamp
      currentWorkflowMeta.value = workflowMeta
      lastSavedAt.value = new Date()

      return {
        success: true,
        data: response,
        id: response.id,
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to save workflow'
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
    currentWorkflowId.value = null
    currentWorkflowMeta.value = {
      name: 'Untitled Workflow',
      description: '',
    }
    lastSavedAt.value = null
    workflowStore.clearCanvas()
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
  const saveAsNew = async (name: string, description?: string) => {
    if (!name?.trim()) {
      ElMessage.error('Please provide a workflow name')
      return { success: false, error: 'Name is required' }
    }

    const previousId = currentWorkflowId.value
    currentWorkflowId.value = null // Force create new

    const result = await saveWorkflow(workflowStore.nodes, workflowStore.edges, {
      showMessage: true,
      meta: { name, description },
    })

    if (!result.success) {
      // Restore previous ID if save failed
      currentWorkflowId.value = previousId
    }

    return result
  }

  /**
   * Auto-save functionality with proper cleanup
   */
  let isAutoSaving = false

  const enableAutoSave = (intervalMs = 60000) => {
    if (intervalMs < 10000) {
      console.warn('Auto-save interval too short, using minimum of 10 seconds')
      intervalMs = 10000
    }

    disableAutoSave() // Clear any existing timer

    autoSaveTimer = setInterval(async () => {
      // Prevent overlapping auto-saves
      if (currentWorkflowId.value && !isSaving.value && !isAutoSaving) {
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
      const workflow = await workflowService.get(id)
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
