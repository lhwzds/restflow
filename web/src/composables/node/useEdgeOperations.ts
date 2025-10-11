import type { Edge } from '@vue-flow/core'
import { computed, type WritableComputedRef } from 'vue'
import { useWorkflowStore } from '../../stores/workflowStore'

export interface EdgeOperations {
  edges: WritableComputedRef<Edge[]>
  addEdge: (edge: Edge) => void
  removeEdge: (edgeId: string) => void
  removeEdges: (edgeIds: string[]) => void
  updateEdge: (edgeId: string, data: Partial<Edge>) => void
}

export function useEdgeOperations(): EdgeOperations {
  const workflowStore = useWorkflowStore()
  
  // Expose reactive references for v-model binding
  const edges = computed({
    get: () => workflowStore.edges,
    set: (value) => { workflowStore.edges = value }
  })

  /**
   * Add a new edge
   */
  const addEdge = (edge: Edge) => {
    workflowStore.addEdge(edge)
  }

  /**
   * Remove an edge
   */
  const removeEdge = (edgeId: string) => {
    workflowStore.removeEdge(edgeId)
  }

  /**
   * Remove multiple edges
   */
  const removeEdges = (edgeIds: string[]) => {
    edgeIds.forEach(id => workflowStore.removeEdge(id))
  }

  /**
   * Update edge data
   */
  const updateEdge = (edgeId: string, data: Partial<Edge>) => {
    const edgeIndex = workflowStore.edges.findIndex(e => e.id === edgeId)
    if (edgeIndex !== -1 && workflowStore.edges[edgeIndex]) {
      workflowStore.edges[edgeIndex] = {
        ...workflowStore.edges[edgeIndex],
        ...data,
        id: workflowStore.edges[edgeIndex].id  // Ensure id is preserved
      } as Edge
      workflowStore.markAsDirty() // Manually mark since we're directly modifying
    }
  }

  return {
    edges,
    addEdge,
    removeEdge,
    removeEdges,
    updateEdge
  }
}