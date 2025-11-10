import { ref, computed, type Ref } from 'vue'
import { ElMessage } from 'element-plus'
import * as triggersApi from '../../api/triggers'
import { SUCCESS_MESSAGES } from '@/constants'
import { isNodeATrigger, type AnyNode } from '../node/useNodeHelpers'

/**
 * Composable for managing workflow-level trigger activation
 * Handles activation/deactivation of ALL triggers in a workflow
 */
export function useWorkflowTrigger(
  workflowId: Readonly<Ref<string | null | undefined>>,
  nodes: Readonly<Ref<AnyNode[]>>,
) {
  const isActive = ref(false)
  const isLoading = ref(false)
  const triggerCount = ref(0)

  const resetTriggerState = () => {
    isActive.value = false
    triggerCount.value = 0
  }

  const validateWorkflowId = (id: string | null | undefined): string | null => {
    if (!id) {
      ElMessage.error('No workflow selected')
      return null
    }
    return id
  }

  const loadTriggerStatus = async () => {
    const currentWorkflowId = workflowId.value
    if (!currentWorkflowId) {
      resetTriggerState()
      return
    }

    try {
      const status = await triggersApi.getTriggerStatus(currentWorkflowId)
      isActive.value = status?.is_active || false
      triggerCount.value = Number(status?.trigger_count || 0)
    } catch (error) {
      console.error('Failed to load trigger status:', error)
      resetTriggerState()
    }
  }

  /**
   * Unified trigger action handler (activate or deactivate)
   */
  const performTriggerAction = async (
    action: 'activate' | 'deactivate',
    apiCall: (id: string) => Promise<void>,
    successState: boolean,
  ) => {
    const currentWorkflowId = validateWorkflowId(workflowId.value)
    if (!currentWorkflowId) return { success: false }

    isLoading.value = true
    try {
      await apiCall(currentWorkflowId)
      isActive.value = successState

      if (action === 'activate') {
        await loadTriggerStatus()
      } else {
        triggerCount.value = 0
      }

      const message =
        action === 'activate'
          ? SUCCESS_MESSAGES.WORKFLOW_ACTIVATED
          : SUCCESS_MESSAGES.WORKFLOW_DEACTIVATED
      ElMessage.success(message)

      return { success: true }
    } catch (error) {
      console.error(`Failed to ${action} workflow:`, error)
      ElMessage.error(`Failed to ${action} workflow triggers`)
      return { success: false, error }
    } finally {
      isLoading.value = false
    }
  }

  const activateWorkflow = () =>
    performTriggerAction('activate', triggersApi.activateWorkflow, true)

  const deactivateWorkflow = () =>
    performTriggerAction('deactivate', triggersApi.deactivateWorkflow, false)

  const toggleActivation = async () => {
    if (isActive.value) {
      return await deactivateWorkflow()
    } else {
      return await activateWorkflow()
    }
  }

  const hasTriggers = computed(() => nodes.value.some(isNodeATrigger))

  const statusText = computed(() => {
    if (!hasTriggers.value) return 'No triggers'
    return isActive.value ? 'Active' : 'Inactive'
  })

  return {
    // State
    isActive,
    isLoading,
    triggerCount,
    hasTriggers,
    statusText,

    // Methods
    loadTriggerStatus,
    activateWorkflow,
    deactivateWorkflow,
    toggleActivation,
  }
}
