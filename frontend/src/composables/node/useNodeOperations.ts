import type { Edge, Node } from '@vue-flow/core'
import { computed, ref } from 'vue'
import { NODE_TYPES } from './useNodeHelpers'
import { useWorkflowStore } from '../../stores/workflowStore'

export interface NodeTemplate {
  type: string
  defaultData: Record<string, any>
  label?: string
  icon?: string
}

export function useNodeOperations() {
  const workflowStore = useWorkflowStore()
  const selectedNodeId = ref<string | null>(null)
  const copiedNode = ref<Node | null>(null)
  
  const nodeIdCounter = ref(Date.now())
  
  const nodes = computed({
    get: () => workflowStore.nodes,
    set: (value) => { workflowStore.nodes = value }
  })

  /**
   * Get node by ID
   */
  const getNodeById = (id: string): Node | undefined => {
    return workflowStore.nodes.find(n => n.id === id)
  }

  /**
   * Get selected node
   */
  const selectedNode = computed(() => {
    return selectedNodeId.value ? getNodeById(selectedNodeId.value) : null
  })

  /**
   * Select a node
   */
  const selectNode = (nodeId: string | null) => {
    selectedNodeId.value = nodeId
  }

  /**
   * Generate unique node ID
   */
  const generateNodeId = (): string => {
    return `node-${nodeIdCounter.value++}-${Math.random().toString(36).substring(2, 9)}`
  }

  /**
   * Create a new node - business logic for node creation
   */
  const createNode = (
    template: NodeTemplate,
    position: { x: number; y: number }
  ): Node => {
    const newNode: Node = {
      id: generateNodeId(),
      type: template.type,
      position,
      data: {
        label: template.label || template.type,
        ...template.defaultData,
      },
    }

    workflowStore.addNode(newNode)
    return newNode
  }

  /**
   * Add node at center of canvas
   */
  const addNodeAtCenter = (template: NodeTemplate): Node => {
    const position = {
      x: 250 + Math.random() * 100,
      y: 150 + Math.random() * 100,
    }
    return createNode(template, position)
  }

  /**
   * Update node data
   */
  const updateNodeData = (nodeId: string, data: Partial<Node['data']>) => {
    workflowStore.updateNodeData(nodeId, data)
  }

  /**
   * Update node position
   */
  const updateNodePosition = (nodeId: string, position: { x: number; y: number }) => {
    const nodeIndex = workflowStore.nodes.findIndex(n => n.id === nodeId)
    if (nodeIndex !== -1) {
      workflowStore.nodes[nodeIndex] = {
        ...workflowStore.nodes[nodeIndex],
        position: { ...position }
      }
      workflowStore.markAsDirty()
    }
  }

  /**
   * Delete node and its connections
   */
  const deleteNode = (nodeId: string) => {
    workflowStore.removeNode(nodeId)
    
    if (selectedNodeId.value === nodeId) {
      selectedNodeId.value = null
    }
  }

  /**
   * Delete multiple nodes
   */
  const deleteNodes = (nodeIds: string[]) => {
    nodeIds.forEach(nodeId => workflowStore.removeNode(nodeId))
    
    if (nodeIds.includes(selectedNodeId.value || '')) {
      selectedNodeId.value = null
    }
  }

  /**
   * Duplicate a node
   */
  const duplicateNode = (
    nodeId: string,
    offset = { x: 50, y: 50 }
  ): Node | null => {
    const node = getNodeById(nodeId)
    if (!node) return null

    const newPosition = {
      x: node.position.x + offset.x,
      y: node.position.y + offset.y,
    }

    const template: NodeTemplate = {
      type: node.type || 'default',
      defaultData: { ...node.data },
      label: node.data.label,
    }

    return createNode(template, newPosition)
  }

  /**
   * Copy node to clipboard (internal)
   */
  const copyNode = (nodeId: string) => {
    const node = getNodeById(nodeId)
    if (node) {
      copiedNode.value = { ...node }
    }
  }

  /**
   * Cut node (copy and delete)
   */
  const cutNode = (nodeId: string) => {
    copyNode(nodeId)
    deleteNode(nodeId)
  }

  /**
   * Paste copied node
   */
  const pasteNode = (position?: { x: number; y: number }): Node | null => {
    if (!copiedNode.value) return null

    const pastePosition = position || {
      x: copiedNode.value.position.x + 50,
      y: copiedNode.value.position.y + 50,
    }

    const template: NodeTemplate = {
      type: copiedNode.value.type || 'default',
      defaultData: { ...copiedNode.value.data },
      label: copiedNode.value.data.label,
    }

    return createNode(template, pastePosition)
  }

  /**
   * Get connected nodes
   */
  const getConnectedNodes = (nodeId: string) => {
    const edges = workflowStore.edges
    const connectedIds = new Set<string>()

    edges.forEach(edge => {
      if (edge.source === nodeId) {
        connectedIds.add(edge.target)
      }
      if (edge.target === nodeId) {
        connectedIds.add(edge.source)
      }
    })

    return Array.from(connectedIds).map(id => getNodeById(id)).filter(Boolean) as Node[]
  }

  /**
   * Get incoming edges for a node
   */
  const getIncomingEdges = (nodeId: string): Edge[] => {
    return workflowStore.edges.filter(edge => edge.target === nodeId)
  }

  /**
   * Get outgoing edges for a node
   */
  const getOutgoingEdges = (nodeId: string): Edge[] => {
    return workflowStore.edges.filter(edge => edge.source === nodeId)
  }

  /**
   * Check if node can be connected to another
   */
  const canConnect = (sourceId: string, targetId: string): boolean => {
    if (sourceId === targetId) return false

    const existingConnection = workflowStore.edges.find(
      edge => edge.source === sourceId && edge.target === targetId
    )
    if (existingConnection) return false

    const wouldCreateCycle = (source: string, target: string): boolean => {
      const visited = new Set<string>()
      const queue = [target]

      while (queue.length > 0) {
        const current = queue.shift()!
        if (current === source) return true
        if (visited.has(current)) continue

        visited.add(current)
        const outgoing = getOutgoingEdges(current)
        queue.push(...outgoing.map(e => e.target))
      }

      return false
    }

    return !wouldCreateCycle(sourceId, targetId)
  }

  /**
   * Validate all nodes
   */
  const validateNodes = (): { valid: boolean; errors: string[] } => {
    const errors: string[] = []

    const hasTrigger = workflowStore.nodes.some(
      node => node.type === NODE_TYPES.MANUAL_TRIGGER
    )
    if (!hasTrigger) {
      errors.push('Workflow must have at least one trigger node')
    }

    workflowStore.nodes.forEach(node => {
      const incoming = getIncomingEdges(node.id)
      const outgoing = getOutgoingEdges(node.id)
      
      if (incoming.length === 0 && outgoing.length === 0 && node.type !== NODE_TYPES.MANUAL_TRIGGER) {
        errors.push(`Node "${node.data.label || node.id}" is not connected`)
      }
    })

    return {
      valid: errors.length === 0,
      errors,
    }
  }

  /**
   * Clear all nodes and edges
   */
  const clearAll = () => {
    workflowStore.clearCanvas()
    selectedNodeId.value = null
    copiedNode.value = null
    nodeIdCounter.value = 1
  }

  return {
    // State
    nodes,
    selectedNodeId,
    selectedNode,
    copiedNode,

    // Methods
    getNodeById,
    selectNode,
    createNode,
    addNodeAtCenter,
    updateNodeData,
    updateNodePosition,
    deleteNode,
    deleteNodes,
    duplicateNode,
    copyNode,
    cutNode,
    pasteNode,
    getConnectedNodes,
    getIncomingEdges,
    getOutgoingEdges,
    canConnect,
    validateNodes,
    clearAll,
  }
}