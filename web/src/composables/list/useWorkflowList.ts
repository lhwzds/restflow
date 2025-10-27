import { useDebounceFn } from '@vueuse/core'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, ref } from 'vue'
import * as workflowsApi from '../../api/workflows'
import type { Workflow } from '@/types/generated/Workflow'
import { useAsyncWorkflowExecution } from '../execution/useAsyncWorkflowExecution'
import { SUCCESS_MESSAGES, ERROR_MESSAGES, VALIDATION_MESSAGES, CONFIRM_MESSAGES, INTERACTION_TIMING } from '@/constants'

export interface FilterOptions {
  searchQuery?: string
  sortBy?: 'name' | 'created_at' | 'updated_at'
  sortOrder?: 'asc' | 'desc'
}

export function useWorkflowList() {
  // State
  const workflows = ref<Workflow[]>([])
  const isLoading = ref(false)
  const searchQuery = ref('')
  const sortBy = ref<'name'>('name')
  const sortOrder = ref<'asc' | 'desc'>('desc')

  /**
   * Load workflows from backend
   */
  const loadWorkflows = async () => {
    isLoading.value = true
    try {
      const response = await workflowsApi.listWorkflows()
      workflows.value = response
      return { success: true, data: workflows.value }
    } catch (error) {
      console.error('Failed to load workflows:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_LOAD('workflows'))
      workflows.value = []
      return { success: false, error }
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Delete workflow
   */
  const deleteWorkflow = async (id: string, confirmMessage?: string) => {
    try {
      await ElMessageBox.confirm(
        confirmMessage || CONFIRM_MESSAGES.DELETE_WORKFLOW,
        'Confirm Delete',
        {
          confirmButtonText: 'Delete',
          cancelButtonText: 'Cancel',
          type: 'warning',
          confirmButtonClass: 'el-button--danger',
        },
      )

      await workflowsApi.deleteWorkflow(id)
      ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_DELETED)

      // Reload to ensure consistency
      await loadWorkflows()

      return { success: true }
    } catch (error) {
      if (error === 'cancel') {
        return { success: false, cancelled: true }
      }

      console.error('Failed to delete workflow:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_DELETE('workflow'))
      return { success: false, error }
    }
  }

  /**
   * Duplicate workflow
   */
  const duplicateWorkflow = async (id: string, newName?: string) => {
    try {
      const sourceWorkflow = await workflowsApi.getWorkflow(id)
      if (!sourceWorkflow) {
        throw new Error('Source workflow not found')
      }

      const duplicateData = {
        ...sourceWorkflow,
        id: `workflow-${Date.now()}-${Math.random().toString(36).substring(2, 11)}`,
        name: newName || `${sourceWorkflow.name} (Copy)`,
      }

      await workflowsApi.createWorkflow(duplicateData)
      const response = duplicateData
      ElMessage.success(SUCCESS_MESSAGES.DUPLICATED('Workflow'))

      await loadWorkflows()

      return { success: true, data: response }
    } catch (error) {
      console.error('Failed to duplicate workflow:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_CREATE('workflow duplicate'))
      return { success: false, error }
    }
  }

  /**
   * Rename workflow
   */
  const renameWorkflow = async (id: string, newName: string) => {
    if (!newName.trim()) {
      ElMessage.error(VALIDATION_MESSAGES.ENTER_NAME)
      return { success: false, error: 'Invalid name' }
    }

    try {
      // Get current workflow data
      const workflow = await workflowsApi.getWorkflow(id)
      if (!workflow) {
        throw new Error('Workflow not found')
      }

      const updatedWorkflow = {
        ...workflow,
        name: newName,
      }

      await workflowsApi.updateWorkflow(updatedWorkflow.id, updatedWorkflow)

      ElMessage.success(SUCCESS_MESSAGES.UPDATED('Workflow name'))

      // Reload to ensure consistency
      await loadWorkflows()

      return { success: true }
    } catch (error) {
      console.error('Failed to rename workflow:', error)
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_UPDATE('workflow name'))
      return { success: false, error }
    }
  }

  /**
   * Execute workflow by ID
   */
  const { startAsyncExecution } = useAsyncWorkflowExecution()
  const executeWorkflow = async (_id: string) => {
    // This function is not used anymore since we use async execution directly
    return startAsyncExecution()
  }

  /**
   * Filtered and sorted workflows
   */
  const filteredWorkflows = computed(() => {
    let result = [...workflows.value]

    // Apply search filter
    if (searchQuery.value) {
      const query = searchQuery.value.toLowerCase()
      result = result.filter((w) => w.name.toLowerCase().includes(query))
    }

    // Apply sorting
    result.sort((a, b) => {
      let compareValue = 0

      // Sort by name (only available sort field)
      compareValue = a.name.localeCompare(b.name)

      return sortOrder.value === 'asc' ? compareValue : -compareValue
    })

    return result
  })

  /**
   * Refresh the workflow list
   */
  const refresh = async () => {
    await loadWorkflows()
  }

  /**
   * Get workflow by ID from local cache
   */
  const getWorkflowById = (id: string) => {
    return workflows.value.find((w) => w.id === id)
  }

  /**
   * Update search query with debouncing
   */
  const setSearchQuery = useDebounceFn((query: string) => {
    searchQuery.value = query
  }, INTERACTION_TIMING.SEARCH_DEBOUNCE)

  /**
   * Update sort options
   */
  const setSortOptions = (
    field: 'name',
    order: 'asc' | 'desc' = 'asc',
  ): void => {
    sortBy.value = field
    sortOrder.value = order
  }

  return {
    // State
    workflows,
    isLoading,
    searchQuery,
    sortBy,
    sortOrder,
    filteredWorkflows,

    // Methods
    loadWorkflows,
    deleteWorkflow,
    duplicateWorkflow,
    renameWorkflow,
    executeWorkflow,
    refresh,
    getWorkflowById,
    setSearchQuery,
    setSortOptions,
  }
}
