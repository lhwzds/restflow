import type { Workflow } from '@/types/generated/Workflow'
import type { Node } from '@vue-flow/core'
import type { Edge } from '@vue-flow/core'

/**
 * Create mock workflow for testing
 */
export function createMockWorkflow(overrides: Partial<Workflow> = {}): Workflow {
  return {
    id: 'test-workflow-id',
    name: 'Test Workflow',
    nodes: [],
    edges: [],
    ...overrides,
  }
}

/**
 * Create mock VueFlow node for testing
 */
export function createMockNode(overrides: Partial<Node> = {}): Node {
  return {
    id: 'node-1',
    type: 'Agent',
    position: { x: 0, y: 0 },
    data: {},
    ...overrides,
  }
}

/**
 * Create mock VueFlow edge for testing
 */
export function createMockEdge(overrides: Partial<Edge> = {}): Edge {
  return {
    id: 'edge-1',
    source: 'node-1',
    target: 'node-2',
    ...overrides,
  }
}

/**
 * Wait for next tick (useful in async tests)
 */
export const nextTick = () => new Promise((resolve) => setTimeout(resolve, 0))
