import type { Edge, Node } from '@vue-flow/core'
import { defineStore } from 'pinia'

interface WorkflowState {
  nodes: Node[]
  edges: Edge[]
  isExecuting: boolean
  executionResult: any
  executionError: string | null
  hasUnsavedChanges: boolean
}

export const useWorkflowStore = defineStore('workflow', {
  state: (): WorkflowState => ({
    nodes: [],
    edges: [],
    isExecuting: false,
    executionResult: null,
    executionError: null,
    hasUnsavedChanges: false,
  }),

  getters: {
    hasNodes(): boolean {
      return this.nodes.length > 0
    },

    canExecute(): boolean {
      return this.hasNodes && !this.isExecuting
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
      this.nodes = []
      this.edges = []
      this.hasUnsavedChanges = false
      this.clearExecutionState()
    },

    updateNodeData(nodeId: string, data: any) {
      const node = this.nodes.find((n) => n.id === nodeId)
      if (node) {
        node.data = { ...node.data, ...data }
        this.hasUnsavedChanges = true
      }
    },

    setExecutionState(isExecuting: boolean, result: any = null, error: string | null = null) {
      this.isExecuting = isExecuting
      this.executionResult = result
      this.executionError = error
    },

    clearExecutionState() {
      this.setExecutionState(false, null, null)
    },

    loadWorkflow(nodes: Node[], edges: Edge[]) {
      this.nodes = nodes || []
      this.edges = edges || []
      this.hasUnsavedChanges = false
      this.clearExecutionState()
    },

    markAsSaved() {
      this.hasUnsavedChanges = false
    },

    markAsDirty() {
      this.hasUnsavedChanges = true
    },
  },
})
