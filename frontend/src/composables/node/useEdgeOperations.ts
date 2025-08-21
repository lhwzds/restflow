import type { Edge } from '@vue-flow/core'
import { useWorkflowStore } from '../../stores/workflowStore'

export function useEdgeOperations() {
  const workflowStore = useWorkflowStore()

  /**
   * Add a new edge
   */
  const addEdge = (edge: Edge) => {
    workflowStore.addEdge(edge)
    // Store automatically marks as dirty
  }

  /**
   * Remove an edge
   */
  const removeEdge = (edgeId: string) => {
    workflowStore.removeEdge(edgeId)
    // Store automatically marks as dirty
  }

  /**
   * Remove multiple edges
   */
  const removeEdges = (edgeIds: string[]) => {
    edgeIds.forEach(id => workflowStore.removeEdge(id))
    // Store automatically marks as dirty
  }

  /**
   * Update edge data
   */
  const updateEdge = (edgeId: string, data: Partial<Edge>) => {
    const edgeIndex = workflowStore.edges.findIndex(e => e.id === edgeId)
    if (edgeIndex !== -1) {
      workflowStore.edges[edgeIndex] = {
        ...workflowStore.edges[edgeIndex],
        ...data
      }
      workflowStore.markAsDirty() // Manually mark since we're directly modifying
    }
  }

  return {
    addEdge,
    removeEdge,
    removeEdges,
    updateEdge
  }
}