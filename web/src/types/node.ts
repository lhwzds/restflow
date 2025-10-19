/**
 * Node plugin system type definitions
 *
 * These types define the metadata structure for nodes in the plugin system,
 * supporting dynamic node registration and configuration.
 */

import type { Position } from '@vue-flow/core'

/**
 * Port (input/output) metadata
 */
export interface PortMetadata {
  /** Unique identifier for this port */
  id: string

  /** Human-readable label */
  label: string

  /** Data type this port accepts/produces (e.g., 'string', 'object', 'any') */
  type: string

  /** Whether this port is required for node execution */
  required?: boolean

  /** Additional description */
  description?: string
}

/**
 * Handle configuration for rendering
 */
export interface HandleConfig {
  /** Unique identifier (optional, auto-generated if not provided) */
  id?: string

  /** Handle type: source (output) or target (input) */
  type: 'source' | 'target'

  /** Position on the node */
  position: Position

  /** Optional label to display on/near the handle */
  label?: string

  /** Custom CSS class for styling */
  className?: string

  /** Port metadata (for validation and type checking) */
  metadata?: PortMetadata
}

/**
 * Node metadata - describes a node type's capabilities
 */
export interface NodeMetadata {
  /** Human-readable name */
  name: string

  /** Description of what this node does */
  description: string

  /** Category for grouping in UI (e.g., 'Action', 'Trigger', 'Logic') */
  category: string

  /** Icon name (from icon library like lucide-vue-next) */
  icon: string

  /** Input ports metadata */
  inputs: PortMetadata[]

  /** Output ports metadata */
  outputs: PortMetadata[]

  /** JSON Schema for node configuration */
  configSchema?: any

  /** Additional tags for search/filtering */
  tags?: string[]
}
