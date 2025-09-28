import { ElMessage, ElMessageBox } from 'element-plus'
import { computed, ref } from 'vue'
import * as triggersApi from '../../api/triggers'
import type { TriggerStatus } from '@/types/generated/TriggerStatus'
import { isNodeATrigger } from '../node/useNodeHelpers'
import { useWorkflowStore } from '../../stores/workflowStore'
import { SUCCESS_MESSAGES, LOADING_MESSAGES } from '@/constants'

export function useWorkflowTriggers() {
  const workflowStore = useWorkflowStore()
  const loading = ref(false)
  // Use Map to manage multiple workflow trigger statuses
  const triggerStatusMap = ref<Map<string, TriggerStatus | null>>(new Map())

  // Check if workflow has trigger nodes - using unified helper
  const hasTriggerNode = computed(() => {
    return workflowStore.nodes.some(isNodeATrigger)
  })

  // Unified error handling helper
  const handleError = (error: any, defaultMessage: string) => {
    const message = 
      error?.response?.data?.error || 
      error?.message || 
      defaultMessage
    console.error(defaultMessage, error)
    ElMessage.error(message)
    return message
  }

  const fetchTriggerStatus = async (workflowId: string) => {
    if (!workflowId) return

    try {
      loading.value = true
      const response = await triggersApi.getTriggerStatus(workflowId)
      if (response) {
        triggerStatusMap.value.set(workflowId, response)
      }
      return response
    } catch (error) {
      handleError(error, 'Failed to fetch trigger status')
      return null
    } finally {
      loading.value = false
    }
  }

  const activateTrigger = async (workflowId: string) => {
    if (!workflowId) {
      ElMessage.warning(LOADING_MESSAGES.SAVE_FIRST)
      return false
    }

    try {
      loading.value = true
      await triggersApi.activateWorkflow(workflowId)
      ElMessage.success(SUCCESS_MESSAGES.TRIGGER_ACTIVATED)

      // Fetch the detailed status
      await fetchTriggerStatus(workflowId)

      return true
    } catch (error) {
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
        },
      )

      if (result === 'confirm') {
        loading.value = true
        await triggersApi.deactivateWorkflow(workflowId)
        ElMessage.success(SUCCESS_MESSAGES.TRIGGER_DEACTIVATED)
        
        // Fetch the updated status
        await fetchTriggerStatus(workflowId)

        return true
      }
      return false
    } catch (error) {
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
    if (status?.is_active) {
      return await deactivateTrigger(workflowId)
    } else {
      return await activateTrigger(workflowId)
    }
  }

  const getTriggerStatus = (workflowId: string): TriggerStatus | undefined => {
    return triggerStatusMap.value.get(workflowId) || undefined
  }

  const fetchAllTriggerStatuses = async (workflowIds: string[]) => {
    // Use Promise.allSettled to handle partial failures gracefully
    const promises = workflowIds.map((id) => fetchTriggerStatus(id))
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
