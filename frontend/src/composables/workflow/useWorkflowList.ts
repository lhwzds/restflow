import { useDebounceFn } from '@vueuse/core'
import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, ref } from 'vue'
import { workflowService } from '../../services/workflowService'
import { useWorkflowExecution } from './useWorkflowExecution'

export interface Workflow {
  id: string
  name: string
  created_at?: string
  updated_at?: string
  nodes?: any[]
  edges?: any[]
}

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
  const sortBy = ref<'name' | 'created_at' | 'updated_at'>('updated_at')
  const sortOrder = ref<'asc' | 'desc'>('desc')

  /**
   * Load workflows from backend
   */
  const loadWorkflows = async () => {
    isLoading.value = true
    try {
      const response = await workflowService.list()
      // Ensure response is an array
      if (response?.status === 'success' && Array.isArray(response.data)) {
        workflows.value = response.data
      } else if (Array.isArray(response)) {
        workflows.value = response
      } else {
        workflows.value = []
        console.warn('Unexpected response format from workflow list API:', response)
      }
      return { success: true, data: workflows.value }
    } catch (error) {
      console.error('Failed to load workflows:', error)
      ElMessage.error('Failed to load workflows')
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
        confirmMessage || 'This will permanently delete the workflow. Continue?',
        'Confirm Delete',
        {
          confirmButtonText: 'Delete',
          cancelButtonText: 'Cancel',
          type: 'warning',
          confirmButtonClass: 'el-button--danger',
        },
      )

      await workflowService.delete(id)
      ElMessage.success('Workflow deleted successfully')

      // Reload to ensure consistency
      await loadWorkflows()

      return { success: true }
    } catch (error) {
      if (error === 'cancel') {
        return { success: false, cancelled: true }
      }

      console.error('Failed to delete workflow:', error)
      ElMessage.error('Failed to delete workflow')
      return { success: false, error }
    }
  }

  /**
   * Duplicate workflow
   */
  const duplicateWorkflow = async (id: string, newName?: string) => {
    try {
      const sourceWorkflow = await workflowService.get(id)
      if (!sourceWorkflow) {
        throw new Error('Source workflow not found')
      }

      const duplicateData = {
        ...sourceWorkflow,
        id: `workflow-${Date.now()}-${Math.random().toString(36).substring(2, 11)}`,
        name: newName || `${sourceWorkflow.name} (Copy)`,
      }

      const response = await workflowService.create(duplicateData)
      ElMessage.success('Workflow duplicated successfully')

      await loadWorkflows()

      return { success: true, data: response }
    } catch (error) {
      console.error('Failed to duplicate workflow:', error)
      ElMessage.error('Failed to duplicate workflow')
      return { success: false, error }
    }
  }

  /**
   * Rename workflow
   */
  const renameWorkflow = async (id: string, newName: string) => {
    if (!newName.trim()) {
      ElMessage.error('Please enter a valid name')
      return { success: false, error: 'Invalid name' }
    }

    try {
      // Get current workflow data
      const workflow = await workflowService.get(id)
      if (!workflow) {
        throw new Error('Workflow not found')
      }

      const updatedWorkflow = {
        ...workflow,
        name: newName,
      }

      await workflowService.save(updatedWorkflow)

      ElMessage.success('Workflow renamed successfully')

      // Reload to ensure consistency
      await loadWorkflows()

      return { success: true }
    } catch (error) {
      console.error('Failed to rename workflow:', error)
      ElMessage.error('Failed to rename workflow')
      return { success: false, error }
    }
  }

  /**
   * Execute workflow by ID
   */
  const { executeWorkflowById } = useWorkflowExecution()
  const executeWorkflow = async (id: string) => {
    return executeWorkflowById(id)
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

      switch (sortBy.value) {
        case 'name':
          compareValue = a.name.localeCompare(b.name)
          break
        case 'created_at':
          compareValue = (a.created_at || '').localeCompare(b.created_at || '')
          break
        case 'updated_at':
          compareValue = (a.updated_at || '').localeCompare(b.updated_at || '')
          break
      }

      return sortOrder.value === 'asc' ? compareValue : -compareValue
    })

    return result
  })

  /**
   * Refresh the workflow list
   */
  const refresh = async () => {
    await loadWorkflows()
    ElMessage.success('Workflow list refreshed')
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
  }, 300)

  /**
   * Update sort options
   */
  const setSortOptions = (
    field: 'name' | 'created_at' | 'updated_at',
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
