import type { Edge, Node } from '@vue-flow/core'
import { defineStore } from 'pinia'
import { NODE_TYPES } from '../constants/nodeTypes'

interface WorkflowState {
  nodes: Node[]
  edges: Edge[]
  isExecuting: boolean
  executionResult: any
  executionError: string | null
}

export const useWorkflowStore = defineStore('workflow', {
  state: (): WorkflowState => ({
    nodes: [],
    edges: [],
    isExecuting: false,
    executionResult: null,
    executionError: null,
  }),

  getters: {
    hasNodes(): boolean {
      return this.nodes.length > 0
    },

    hasTriggerNode(): boolean {
      return this.nodes.some((node) => node.type === NODE_TYPES.MANUAL_TRIGGER)
    },

    canExecute(): boolean {
      return this.hasNodes && !this.isExecuting
    },
  },

  actions: {
    // Add a single node
    addNode(node: Node) {
      this.nodes.push(node)
    },

    // Remove a node and its edges
    removeNode(nodeId: string) {
      this.nodes = this.nodes.filter((n) => n.id !== nodeId)
      this.edges = this.edges.filter((e) => e.source !== nodeId && e.target !== nodeId)
    },

    // Add an edge
    addEdge(edge: Edge) {
      this.edges.push(edge)
    },


    // Clear all nodes and edges
    clearCanvas() {
      this.nodes = []
      this.edges = []
      this.clearExecutionState()
    },

    // Update node data
    updateNodeData(nodeId: string, data: any) {
      const node = this.nodes.find((n) => n.id === nodeId)
      if (node) {
        node.data = { ...node.data, ...data }
      }
    },

    // Set execution state
    setExecutionState(isExecuting: boolean, result: any = null, error: string | null = null) {
      this.isExecuting = isExecuting
      this.executionResult = result
      this.executionError = error
    },

    // Clear execution state
    clearExecutionState() {
      this.setExecutionState(false, null, null)
    },

    // Load workflow data
    loadWorkflow(nodes: Node[], edges: Edge[]) {
      this.nodes = nodes || []
      this.edges = edges || []
      this.clearExecutionState()
    },
  },
})
