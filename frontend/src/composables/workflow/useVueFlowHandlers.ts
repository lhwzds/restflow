import { useWorkflowStore } from '../../stores/workflowStore'

export function useVueFlowHandlers() {
  const workflowStore = useWorkflowStore()

  const handleEdgesChange = (changes: any[]) => {
    const hasRemoval = changes.some((change) => change.type === 'remove')

    if (hasRemoval) {
      workflowStore.markAsDirty()
    }
  }

  const handleNodesChange = (changes: any[]) => {
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
