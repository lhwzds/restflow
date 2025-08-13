import { defineStore } from 'pinia'
import type { Node, Edge } from '@vue-flow/core'
import { workflowService } from '../services/workflowService'

interface WorkflowState {
  nodes: Node[]
  edges: Edge[]
  isExecuting: boolean
  executionResult: any
  nodeIdCounter: number
}

export const useWorkflowStore = defineStore('workflow', {
  state: (): WorkflowState => ({
    nodes: [],
    edges: [],
    isExecuting: false,
    executionResult: null,
    nodeIdCounter: 1
  }),

  getters: {
    hasNodes(): boolean {
      return this.nodes.length > 0
    },
    
    hasTriggerNode(): boolean {
      return this.nodes.some(node => node.type === 'manual-trigger')
    }
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
    createNode(template: any, position: { x: number, y: number }) {
      const newNode: Node = {
        id: `node-${this.nodeIdCounter++}`,
        type: template.type,
        position,
        data: { ...template.defaultData }
      }
      
      this.addNode(newNode)
      return newNode
    },

    // Remove a node
    removeNode(nodeId: string) {
      const index = this.nodes.findIndex(n => n.id === nodeId)
      if (index > -1) {
        this.nodes.splice(index, 1)
        // Also remove related edges
        this.edges = this.edges.filter(
          e => e.source !== nodeId && e.target !== nodeId
        )
      }
    },

    // Add an edge
    addEdge(edge: Edge) {
      this.edges.push(edge)
    },

    // Remove edges
    removeEdges(edgeIds: string[]) {
      this.edges = this.edges.filter(e => !edgeIds.includes(e.id))
    },

    // Clear all nodes and edges
    clearCanvas() {
      this.nodes = []
      this.edges = []
      this.nodeIdCounter = 1
    },

    // Execute workflow
    async executeWorkflow() {
      console.log('Executing workflow with nodes:', this.nodes)
      console.log('Edges:', this.edges)
      
      if (this.nodes.length === 0) {
        throw new Error('No nodes to execute')
      }

      this.isExecuting = true
      this.executionResult = null

      try {
        const result = await workflowService.execute(this.nodes, this.edges)
        this.executionResult = result
        return result
      } finally {
        this.isExecuting = false
      }
    }
  }
})