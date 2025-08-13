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

// Convert VueFlow format to backend format
export const convertToBackendFormat = (nodes: Node[], edges: Edge[]) => {
  const nodeTypeMap: Record<string, string> = {
    agent: 'Agent',
    http: 'HttpRequest',
    'manual-trigger': 'ManualTrigger',
  }

  const workflowNodes = nodes.map((node) => ({
    id: node.id,
    node_type: nodeTypeMap[node.type || ''],
    config: node.data || {},
  }))

  const workflowEdges = edges.map((edge) => ({
    from: edge.source,
    to: edge.target,
  }))

  return {
    id: `workflow-${Date.now()}`,
    name: 'My Workflow',
    nodes: workflowNodes,
    edges: workflowEdges,
  }
}

// API methods
export const workflowService = {
  // Execute workflow
  async execute(nodes: Node[], edges: Edge[]) {
    const workflow = convertToBackendFormat(nodes, edges)
    const response = await apiClient.post('/execute', workflow)
    return response.data
  },

  // Save workflow
  async create(nodes: Node[], edges: Edge[]) {
    const workflow = convertToBackendFormat(nodes, edges)
    const response = await apiClient.post('/create', workflow)
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

  // Update workflow
  async update(id: string, nodes: Node[], edges: Edge[]) {
    const workflow = convertToBackendFormat(nodes, edges)
    const response = await apiClient.put(`/update/${id}`, workflow)
    return response.data
  },

  // Delete workflow
  async delete(id: string) {
    const response = await apiClient.delete(`/delete/${id}`)
    return response.data
  },
}
