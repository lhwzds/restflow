/**
 * Schema Generator Utility
 *
 * Generates VariableField structures from example output values.
 * Uses backend-generated types (via ts-rs) as the source of truth.
 */

import type { VariableField } from '@/composables/variables/useAvailableVariables'
import { NODE_OUTPUT_EXAMPLES } from '@/constants/node/output-examples'

/**
 * Parse a JSON value into VariableField structure
 * Recursively processes objects and arrays to build field hierarchy
 */
function parseValueToFields(value: any, basePath: string): VariableField[] {
  if (value === null || value === undefined) {
    return []
  }

  if (typeof value !== 'object') {
    return [
      {
        name: '',
        type: typeof value,
        path: basePath,
        value,
      },
    ]
  }

  if (Array.isArray(value)) {
    const itemFields: VariableField[] = []

    // For arrays, show first item as example
    if (value.length > 0) {
      const firstItem = value[0]
      const children = parseValueToFields(firstItem, `${basePath}[0]`)

      itemFields.push({
        name: '[0]',
        type: 'array-item',
        path: `${basePath}[0]`,
        value: firstItem,
        children,
      })
    }

    return [
      {
        name: '',
        type: 'array',
        path: basePath,
        value,
        children: itemFields,
      },
    ]
  }

  const fields: VariableField[] = []
  for (const [key, val] of Object.entries(value)) {
    const fieldPath = basePath ? `${basePath}.${key}` : key

    if (typeof val === 'object' && val !== null) {
      const children = parseValueToFields(val, fieldPath)
      fields.push({
        name: key,
        type: Array.isArray(val) ? 'array' : 'object',
        path: fieldPath,
        value: val,
        children,
      })
    } else {
      fields.push({
        name: key,
        type: typeof val,
        path: fieldPath,
        value: val,
      })
    }
  }

  return fields
}

/**
 * Get output schema for a node type based on example data
 * Returns VariableField array with structure and example values
 */
export function getNodeOutputSchema(nodeType: string): VariableField[] {
  const example = NODE_OUTPUT_EXAMPLES[nodeType]
  if (!example) {
    return []
  }

  return parseValueToFields(example, '')
}

/**
 * Check if a node type has an output schema defined
 */
export function hasNodeOutputSchema(nodeType: string): boolean {
  return nodeType in NODE_OUTPUT_EXAMPLES
}
