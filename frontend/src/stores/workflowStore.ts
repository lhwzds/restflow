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
      console.log('Store: Adding node', node)
      this.nodes.push(node)
      console.log('Store: Total nodes', this.nodes.length)
    },

    // Add multiple nodes
    addNodes(newNodes: Node[]) {
      this.nodes.push(...newNodes)
    },

    // Remove a node
    removeNode(nodeId: string) {
      const index = this.nodes.findIndex((n) => n.id === nodeId)
      if (index > -1) {
        this.nodes.splice(index, 1)
        // Also remove related edges
        this.edges = this.edges.filter((e) => e.source !== nodeId && e.target !== nodeId)
      }
    },

    // Remove multiple nodes
    removeNodes(nodeIds: string[]) {
      // Remove all nodes in one operation
      this.nodes = this.nodes.filter((n) => !nodeIds.includes(n.id))
      // Remove all related edges
      this.edges = this.edges.filter(
        (e) => !nodeIds.includes(e.source) && !nodeIds.includes(e.target),
      )
    },

    // Add an edge
    addEdge(edge: Edge) {
      this.edges.push(edge)
    },

    // Remove edges
    removeEdges(edgeIds: string[]) {
      this.edges = this.edges.filter((e) => !edgeIds.includes(e.id))
    },

    // Clear all nodes and edges
    clearCanvas() {
      this.nodes = []
      this.edges = []
      this.executionError = null
      this.executionResult = null
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
      this.executionResult = null
      this.executionError = null
    },

    // Load workflow data
    loadWorkflow(nodes: Node[], edges: Edge[]) {
      this.nodes = nodes || []
      this.edges = edges || []
      this.executionError = null
      this.executionResult = null
    },

    // Set nodes
    setNodes(nodes: Node[]) {
      this.nodes = nodes
    },

    // Set edges
    setEdges(edges: Edge[]) {
      this.edges = edges
    },

    // Update entire workflow
    updateWorkflow(nodes: Node[], edges: Edge[]) {
      this.nodes = nodes
      this.edges = edges
    },
  },
})
