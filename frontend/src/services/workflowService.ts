import type { Edge, Node } from '@vue-flow/core'
import axios from 'axios'

const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000'

// Create axios instance
const apiClient = axios.create({
  baseURL: `${API_BASE_URL}/api/workflow`,
  headers: {
    'Content-Type': 'application/json',
  },
})

// Workflow metadata interface
export interface WorkflowMeta {
  id: string
  name: string
  description?: string
  created_at?: string
  updated_at?: string
}

// Convert backend format to VueFlow format
export const convertFromBackendFormat = (workflow: any) => {
  const nodes =
    workflow.nodes?.map((node: any) => ({
      id: node.id,
      type: node.node_type,
      position: node.position || { x: 100 + Math.random() * 500, y: 100 + Math.random() * 400 },
      data: node.config || {},
    })) || []

  const edges =
    workflow.edges?.map((edge: any) => ({
      id: `e${edge.from}-${edge.to}`,
      source: edge.from,
      target: edge.to,
      animated: true,
    })) || []

  return { nodes, edges }
}

// Convert VueFlow format to backend format
export const convertToBackendFormat = (
  nodes: Node[],
  edges: Edge[],
  meta?: Partial<WorkflowMeta>,
) => {
  const workflowNodes = nodes.map((node) => ({
    id: node.id,
    node_type: node.type,
    config: node.data || {},
    position: node.position ? { x: node.position.x, y: node.position.y } : undefined,
  }))

  const workflowEdges = edges.map((edge) => ({
    from: edge.source,
    to: edge.target,
  }))

  return {
    id: meta?.id || `workflow-${Date.now()}`,
    name: meta?.name || 'My Workflow',
    description: meta?.description,
    nodes: workflowNodes,
    edges: workflowEdges,
  }
}

// API methods
export const workflowService = {
  async execute(
    workflowOrId: string | { nodes: Node[]; edges: Edge[]; meta?: Partial<WorkflowMeta> },
  ) {
    if (typeof workflowOrId === 'string') {
      const response = await apiClient.post(`/execute/${workflowOrId}`)
      return response.data
    }
    const workflow = convertToBackendFormat(
      workflowOrId.nodes,
      workflowOrId.edges,
      workflowOrId.meta,
    )
    const response = await apiClient.post('/execute', workflow)
    return response.data
  },

  // Get workflow by ID
  async get(id: string) {
    const response = await apiClient.get(`/get/${id}`)
    return response.data
  },

  // List all workflows
  async list() {
    const response = await apiClient.get('/list')
    return response.data
  },

  // Save workflow
  async save(data: any) {
    const id = data.id
    const endpoint = id ? `/update/${id}` : '/create'
    const method = id ? 'put' : 'post'

    // Check if nodes are already in backend format
    const isBackendFormat = data.nodes?.[0]?.node_type !== undefined

    const payload =
      data.nodes && Array.isArray(data.nodes) && !isBackendFormat
        ? convertToBackendFormat(data.nodes, data.edges, data)
        : data

    const response = await apiClient[method](endpoint, payload)
    return response.data
  },

  // Delete workflow
  async delete(id: string) {
    const response = await apiClient.delete(`/delete/${id}`)
    return response.data
  },
}
