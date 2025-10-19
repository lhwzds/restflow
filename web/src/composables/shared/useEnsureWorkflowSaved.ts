import { ElMessage } from 'element-plus'
import { useWorkflowStore } from '@/stores/workflowStore'
import { useWorkflowPersistence } from '../persistence/useWorkflowPersistence'
import { ERROR_MESSAGES } from '@/constants'

/**
 * Composable to ensure a workflow is saved before performing operations
 * Useful for operations that require a workflow ID (test, execute, etc.)
 */
export function useEnsureWorkflowSaved() {
  const workflowStore = useWorkflowStore()
  const { saveWorkflow } = useWorkflowPersistence()

  /**
   * Ensure the current workflow is saved
   * If not saved, attempts to save it first
   *
   * @param options - Configuration options
   * @param options.showMessage - Whether to show success/error messages
   * @returns Result with success flag and workflow ID
   */
  const ensureSaved = async (options?: { showMessage?: boolean }) => {
    if (workflowStore.currentWorkflowId) {
      return { success: true, id: workflowStore.currentWorkflowId }
    }

    const result = await saveWorkflow(
      workflowStore.nodes,
      workflowStore.edges,
      {
        showMessage: options?.showMessage ?? false,
        meta: { name: workflowStore.currentWorkflowName || 'Untitled Workflow' },
      }
    )

    if (!result.success) {
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_SAVE('workflow'))
      return { success: false, id: null }
    }

    return { success: true, id: result.id || null }
  }

  return { ensureSaved }
}
