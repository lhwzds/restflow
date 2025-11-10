import type { Edge, Node } from '@vue-flow/core'
import { defineStore } from 'pinia'

interface WorkflowState {
  nodes: Node[]
  edges: Edge[]
  hasUnsavedChanges: boolean
  currentWorkflowId: string | null
  currentWorkflowName: string
}

export const useWorkflowStore = defineStore('workflow', {
  state: (): WorkflowState => ({
    nodes: [],
    edges: [],
    hasUnsavedChanges: false,
    currentWorkflowId: null,
    currentWorkflowName: 'Untitled Workflow',
  }),

  getters: {
    hasNodes(): boolean {
      return this.nodes.length > 0
    },
  },

  actions: {
    addNode(node: Node) {
      this.nodes.push(node)
      this.hasUnsavedChanges = true
    },

    removeNode(nodeId: string) {
      this.nodes = this.nodes.filter((n) => n.id !== nodeId)
      this.edges = this.edges.filter((e) => e.source !== nodeId && e.target !== nodeId)
      this.hasUnsavedChanges = true
    },

    addEdge(edge: Edge) {
      this.edges.push(edge)
      this.hasUnsavedChanges = true
    },

    removeEdge(edgeId: string) {
      this.edges = this.edges.filter((e) => e.id !== edgeId)
      this.hasUnsavedChanges = true
    },

    clearCanvas() {
      // Only clear canvas data, preserve workflow metadata
      this.nodes = []
      this.edges = []
      this.hasUnsavedChanges = false
    },

    resetWorkflow() {
      // Full reset including metadata
      this.nodes = []
      this.edges = []
      this.hasUnsavedChanges = false
      this.currentWorkflowId = null
      this.currentWorkflowName = 'Untitled Workflow'
    },

    updateNodeData(nodeId: string, data: any, markDirty = true) {
      const node = this.nodes.find((n) => n.id === nodeId)
      if (node) {
        node.data = { ...node.data, ...data }
        if (markDirty) {
          this.hasUnsavedChanges = true
        }
      }
    },

    loadWorkflow(nodes: Node[], edges: Edge[]) {
      this.nodes = nodes || []
      this.edges = edges || []
      this.hasUnsavedChanges = false
    },

    markAsSaved() {
      this.hasUnsavedChanges = false
    },

    markAsDirty() {
      this.hasUnsavedChanges = true
    },

    setWorkflowMetadata(id: string | null, name: string) {
      this.currentWorkflowId = id
      this.currentWorkflowName = name
    },
  },
})
