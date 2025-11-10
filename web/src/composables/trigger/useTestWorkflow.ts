import { ref, computed } from 'vue'
import { ElMessage } from 'element-plus'
import { useEnsureWorkflowSaved } from '@/composables/shared/useEnsureWorkflowSaved'
import { useExecutionMonitor } from '@/composables/execution/useAsyncWorkflowExecution'
import { useExecutionStore } from '@/stores/executionStore'
import * as triggersApi from '@/api/triggers'
import { ERROR_MESSAGES } from '@/constants'

export function useTestWorkflow(triggerNodeId?: string) {
  const { ensureSaved } = useEnsureWorkflowSaved()
  const { isExecuting, monitorExecution } = useExecutionMonitor()
  const executionStore = useExecutionStore()
  const isSubmitting = ref(false)

  const isButtonDisabled = computed(() => isExecuting.value || isSubmitting.value)

  const buttonLabel = computed(() => {
    if (isExecuting.value) {
      return 'Executing...'
    }
    if (isSubmitting.value) {
      return 'Starting...'
    }
    return 'Test RestFlow'
  })

  const buttonTooltip = computed(() => {
    if (isExecuting.value) {
      return 'Workflow execution in progress'
    }
    if (isSubmitting.value) {
      return 'Queuing test execution...'
    }
    return 'Test this workflow manually'
  })

  const testWorkflow = async () => {
    const { success, id } = await ensureSaved()
    if (!success || !id) return

    isSubmitting.value = true
    try {
      const response = await triggersApi.testWorkflow(id)
      const executionId = response?.execution_id

      if (!executionId) {
        throw new Error('Missing execution ID')
      }

      monitorExecution(executionId, {
        label: 'Test workflow',
      })

      if (triggerNodeId) {
        executionStore.setNodeResult(triggerNodeId, {
          nodeId: triggerNodeId,
          status: 'Running',
          input: {},
          output: undefined,
          error: undefined,
          startTime: Date.now(),
          endTime: undefined,
          executionTime: undefined,
        })
      }
    } catch (error) {
      ElMessage.error(ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED)
    } finally {
      isSubmitting.value = false
    }
  }

  return {
    testWorkflow,
    isSubmitting,
    isExecuting,
    isButtonDisabled,
    buttonLabel,
    buttonTooltip,
  }
}
