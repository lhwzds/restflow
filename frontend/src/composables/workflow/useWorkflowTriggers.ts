import { ref, computed } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { workflowService } from '../../services/workflowService'
import { useWorkflowStore } from '../../stores/workflowStore'
import { isNodeATrigger } from '../../constants/nodeTypes'

export interface TriggerStatus {
  active: boolean
  trigger_type?: string
  webhook_url?: string
  schedule?: string
  last_triggered?: string
  error?: string
}

export function useWorkflowTriggers() {
  const workflowStore = useWorkflowStore()
  const loading = ref(false)
  // Use Map to manage multiple workflow trigger statuses
  const triggerStatusMap = ref<Map<string, TriggerStatus>>(new Map())

  // Check if workflow has trigger nodes - using unified helper
  const hasTriggerNode = computed(() => {
    return workflowStore.nodes.some(isNodeATrigger)
  })

  // Unified error handling helper
  const handleError = (error: any, defaultMessage: string) => {
    const message = error?.response?.data?.error || error?.message || defaultMessage
    console.error(defaultMessage, error)
    ElMessage.error(message)
    return message
  }

  const fetchTriggerStatus = async (workflowId: string) => {
    if (!workflowId) return

    try {
      loading.value = true
      const response = await workflowService.getTriggerStatus(workflowId)
      if (response) {
        triggerStatusMap.value.set(workflowId, response)
      }
      return response
    } catch (error: any) {
      handleError(error, 'Failed to fetch trigger status')
      return null
    } finally {
      loading.value = false
    }
  }

  const activateTrigger = async (workflowId: string) => {
    if (!workflowId) {
      ElMessage.warning('Please save the workflow first')
      return false
    }

    try {
      loading.value = true
      const response = await workflowService.activate(workflowId)
      ElMessage.success('Trigger activated successfully')
      
      // Always update status to active after successful activation
      triggerStatusMap.value.set(workflowId, { active: true })
      // Fetch the detailed status
      await fetchTriggerStatus(workflowId)
      
      return true
    } catch (error: any) {
      handleError(error, 'Failed to activate trigger')
      return false
    } finally {
      loading.value = false
    }
  }

  const deactivateTrigger = async (workflowId: string) => {
    if (!workflowId) return false

    try {
      const result = await ElMessageBox.confirm(
        'Are you sure you want to deactivate the trigger? The workflow will not be triggered automatically after deactivation.',
        'Deactivate Trigger',
        {
          confirmButtonText: 'Confirm',
          cancelButtonText: 'Cancel',
          type: 'warning',
        }
      )

      if (result === 'confirm') {
        loading.value = true
        await workflowService.deactivate(workflowId)
        ElMessage.success('Trigger deactivated successfully')
        
        // Update status
        triggerStatusMap.value.set(workflowId, { active: false })
        
        return true
      }
      return false
    } catch (error: any) {
      if (error !== 'cancel') {
        handleError(error, 'Failed to deactivate trigger')
      }
      return false
    } finally {
      loading.value = false
    }
  }


  const toggleTriggerStatus = async (workflowId: string) => {
    const currentStatus = triggerStatusMap.value.get(workflowId)
    if (!currentStatus) {
      await fetchTriggerStatus(workflowId)
    }

    const status = triggerStatusMap.value.get(workflowId)
    if (status?.active) {
      return await deactivateTrigger(workflowId)
    } else {
      return await activateTrigger(workflowId)
    }
  }

  const getTriggerStatus = (workflowId: string): TriggerStatus => {
    return triggerStatusMap.value.get(workflowId) || { active: false }
  }

  const fetchAllTriggerStatuses = async (workflowIds: string[]) => {
    // Use Promise.allSettled to handle partial failures gracefully
    const promises = workflowIds.map(id => fetchTriggerStatus(id))
    await Promise.allSettled(promises)
  }

  return {
    loading,
    triggerStatusMap,
    hasTriggerNode,
    fetchTriggerStatus,
    fetchAllTriggerStatuses,
    getTriggerStatus,
    activateTrigger,
    deactivateTrigger,
    toggleTriggerStatus,
  }
}