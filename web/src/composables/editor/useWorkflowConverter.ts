import type { Edge as BackendEdge } from '@/types/generated/Edge'
import type { Node as BackendNode } from '@/types/generated/Node'
import type { NodeType } from '@/types/generated/NodeType'
import type { Workflow } from '@/types/generated/Workflow'
import type { Edge as VueFlowEdge, Node as VueFlowNode } from '@vue-flow/core'

export function useWorkflowConverter() {
  const convertFromBackendFormat = (
    workflow: Workflow,
  ): { nodes: VueFlowNode[]; edges: VueFlowEdge[] } => {
    const nodes: VueFlowNode[] =
      workflow.nodes?.map((node) => {
        // Extract data from nested structure for all nodes
        // Backend format: {"type": "Python", "data": {"code": "...", "dependencies": [...]}}
        // Frontend format: {"code": "...", "dependencies": [...]}
        // If config has 'data' field (new format), use it; otherwise use entire config (backward compatibility)
        const data = node.config?.data ? node.config.data : node.config || {}

        return {
          id: node.id,
          type: node.node_type,
          position: node.position || { x: 100 + Math.random() * 500, y: 100 + Math.random() * 400 },
          data,
        }
      }) || []

    const edges: VueFlowEdge[] =
      workflow.edges?.map((edge) => ({
        id: `e${edge.from}-${edge.to}`,
        source: edge.from,
        target: edge.to,
        animated: true,
      })) || []

    return { nodes, edges }
  }

  const convertToBackendFormat = (
    nodes: VueFlowNode[],
    edges: VueFlowEdge[],
    meta?: Partial<Workflow>,
  ): Workflow => {
    const workflowNodes: BackendNode[] = nodes.map((node) => {
      // Wrap flattened data into typed structure for all nodes
      // Frontend format: {"code": "...", "dependencies": [...]}
      // Backend format: {"type": "Python", "data": {"code": "...", "dependencies": [...]}}
      // This matches Rust's tagged enum format: #[serde(tag = "type", content = "data")]
      const config = {
        type: node.type,
        data: node.data || {},
      }

      return {
        id: node.id,
        node_type: node.type as NodeType,
        config,
        position: node.position ? { x: node.position.x, y: node.position.y } : null,
      }
    })

    const workflowEdges: BackendEdge[] = edges.map((edge) => ({
      from: edge.source,
      to: edge.target,
    }))

    return {
      id: meta?.id || `workflow-${Date.now()}`,
      name: meta?.name || 'My Workflow',
      nodes: workflowNodes,
      edges: workflowEdges,
    }
  }

  return {
    convertFromBackendFormat,
    convertToBackendFormat,
  }
}
