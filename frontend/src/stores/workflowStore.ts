import type { Edge, Node } from '@vue-flow/core'
import { defineStore } from 'pinia'
import { workflowService } from '../services/workflowService'
import { NODE_TYPES } from '../constants/nodeTypes'

interface WorkflowState {
  nodes: Node[]
  edges: Edge[]
  isExecuting: boolean
  executionResult: any
  executionError: string | null
  nodeIdCounter: number
}

export const useWorkflowStore = defineStore('workflow', {
  state: (): WorkflowState => ({
    nodes: [],
    edges: [],
    isExecuting: false,
    executionResult: null,
    executionError: null,
    nodeIdCounter: 1,
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

    // Create and add a new node from template
    createNode(template: any, position: { x: number; y: number }) {
      const newNode: Node = {
        id: `node-${this.nodeIdCounter++}`,
        type: template.type,
        position,
        data: { ...template.defaultData },
      }

      this.addNode(newNode)
      return newNode
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

    // Remove multiple nodes (batch operation)
    removeNodes(nodeIds: string[]) {
      // Remove all nodes in one operation
      this.nodes = this.nodes.filter((n) => !nodeIds.includes(n.id))
      // Remove all related edges
      this.edges = this.edges.filter(
        (e) => !nodeIds.includes(e.source) && !nodeIds.includes(e.target)
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
      this.nodeIdCounter = 1
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

    // Duplicate a node
    duplicateNode(nodeId: string, offset = { x: 50, y: 50 }) {
      const node = this.nodes.find((n) => n.id === nodeId)
      if (!node) return null

      const newPosition = {
        x: node.position.x + offset.x,
        y: node.position.y + offset.y,
      }

      return this.createNode({ type: node.type, defaultData: node.data }, newPosition)
    },

    // Execute workflow with error handling
    async executeWorkflow() {
      if (!this.canExecute) {
        this.executionError = 'Cannot execute: no nodes or already executing'
        throw new Error(this.executionError)
      }

      console.log('Executing workflow with nodes:', this.nodes)
      console.log('Edges:', this.edges)

      this.isExecuting = true
      this.executionResult = null
      this.executionError = null

      try {
        const result = await workflowService.execute(this.nodes, this.edges)
        this.executionResult = result
        console.log('Workflow executed successfully:', result)
        return result
      } catch (error) {
        this.executionError = error instanceof Error ? error.message : 'Unknown error occurred'
        console.error('Workflow execution failed:', error)
        throw error
      } finally {
        this.isExecuting = false
      }
    },

    // Clear execution error
    clearExecutionError() {
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
