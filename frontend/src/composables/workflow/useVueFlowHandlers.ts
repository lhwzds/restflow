import { useWorkflowStore } from '../../stores/workflowStore'
import type { EdgeChange, NodeChange } from '@vue-flow/core'

export function useVueFlowHandlers() {
  const workflowStore = useWorkflowStore()

  const handleEdgesChange = (changes: EdgeChange[]) => {
    const hasRemoval = changes.some((change) => change.type === 'remove')

    if (hasRemoval) {
      workflowStore.markAsDirty()
    }
  }

  const handleNodesChange = (changes: NodeChange[]) => {
    const hasRemoval = changes.some((change) => change.type === 'remove')

    if (hasRemoval) {
      workflowStore.markAsDirty()
    }
  }

  return {
    handleEdgesChange,
    handleNodesChange,
  }
}
