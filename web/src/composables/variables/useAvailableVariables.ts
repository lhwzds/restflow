import { ref, computed, watch, type Ref } from 'vue'
import { useWorkflowStore } from '@/stores/workflowStore'
import { useExecutionStore } from '@/stores/executionStore'
import { getNodeOutputSchema } from '@/utils/schemaGenerator'

export interface VariableField {
  name: string
  type: string
  path: string
  value?: any
  children?: VariableField[]
}

export interface VariableNode {
  id: string
  type: string
  label: string
  fields: VariableField[]
}

export interface AvailableVariables {
  trigger: VariableField[]
  nodes: VariableNode[]
  vars: VariableField[]
  config: VariableField[]
}

/**
 * Composable for managing available variables for a specific node
 * Provides autocomplete data for ExpressionInput components
 */
export function useAvailableVariables(currentNodeId: Readonly<Ref<string | null>>) {
  const workflowStore = useWorkflowStore()
  const executionStore = useExecutionStore()

  const availableVariables = ref<AvailableVariables>({
    trigger: [],
    nodes: [],
    vars: [],
    config: []
  })

  const getUpstreamNodes = (nodeId: string): string[] => {
    const upstreamIds: string[] = []
    const visited = new Set<string>()

    const traverse = (currentId: string) => {
      if (visited.has(currentId)) return
      visited.add(currentId)

      const incomingEdges = workflowStore.edges.filter((edge: any) => edge.target === currentId)

      for (const edge of incomingEdges) {
        upstreamIds.push(edge.source)
        traverse(edge.source)
      }
    }

    traverse(nodeId)
    return upstreamIds
  }

  const parseValueToFields = (value: any, basePath: string): VariableField[] => {
    if (value === null || value === undefined) {
      return []
    }

    if (typeof value !== 'object') {
      return [{
        name: '',
        type: typeof value,
        path: basePath,
        value
      }]
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
          children
        })
      }

      return [{
        name: '',
        type: 'array',
        path: basePath,
        value,
        children: itemFields
      }]
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
          children
        })
      } else {
        fields.push({
          name: key,
          type: typeof val,
          path: fieldPath,
          value: val
        })
      }
    }

    return fields
  }

  const addNamespaceToField = (field: VariableField, namespace: string): VariableField => {
    const newField = { ...field }
    newField.path = `${namespace}.${field.path}`

    if (field.children && field.children.length > 0) {
      newField.children = field.children.map(child => addNamespaceToField(child, namespace))
    }

    return newField
  }

  const loadVariablesFromExecution = () => {
    if (!currentNodeId.value) {
      availableVariables.value = { trigger: [], nodes: [], vars: [], config: [] }
      return
    }

    const upstreamNodeIds = getUpstreamNodes(currentNodeId.value)

    const nodeResults = executionStore.nodeResults

    if (nodeResults.size === 0) {
      // No execution yet - show schema-based field structure with example values
      availableVariables.value = {
        trigger: [],
        nodes: upstreamNodeIds.map(id => {
          const node = workflowStore.nodes.find((n: any) => n.id === id)
          const nodeType = node?.type || 'unknown'

          const schemaFields = getNodeOutputSchema(nodeType)
          const fields = schemaFields.map(field => addNamespaceToField(field, `node.${id}`))

          return {
            id,
            type: nodeType,
            label: node?.id || id,
            fields
          }
        }),
        vars: [],
        config: []
      }
      return
    }

    // For now, trigger data would come from the first trigger node's input
    const triggerNodes = workflowStore.nodes.filter((n: any) =>
      n.type === 'ManualTrigger' || n.type === 'WebhookTrigger' || n.type === 'ScheduleTrigger'
    )
    const triggerNode = triggerNodes[0]
    const triggerResult = triggerNode ? nodeResults.get(triggerNode.id) : null
    const triggerData = triggerResult?.input
    const triggerFields = triggerData
      ? parseValueToFields(triggerData, 'trigger.payload')
      : []

    const nodeVariables: VariableNode[] = upstreamNodeIds.map(nodeId => {
      const node = workflowStore.nodes.find((n: any) => n.id === nodeId)
      const nodeResult = nodeResults.get(nodeId)
      const nodeOutput = nodeResult?.output
      const nodeType = node?.type || 'unknown'

      let fields: VariableField[]
      if (nodeOutput) {
        fields = parseValueToFields(nodeOutput, `node.${nodeId}`)
      } else {
        const schemaFields = getNodeOutputSchema(nodeType)
        fields = schemaFields.map(field => addNamespaceToField(field, `node.${nodeId}`))
      }

      return {
        id: nodeId,
        type: nodeType,
        label: nodeId,
        fields
      }
    })

    // For now, we don't have var.* and config.* in the current execution model
    // These would need to be added to the backend context system
    const varFields: VariableField[] = []
    const configFields: VariableField[] = []

    availableVariables.value = {
      trigger: triggerFields,
      nodes: nodeVariables,
      vars: varFields,
      config: configFields
    }
  }

  const generateVariablePath = (field: VariableField): string => {
    return `{{${field.path}}}`
  }

  // Flattens nested variable structure for autocomplete dropdown
  const getAllVariablePaths = computed<string[]>(() => {
    const paths: string[] = []

    const extractPaths = (fields: VariableField[]) => {
      for (const field of fields) {
        if (field.path) {
          paths.push(field.path)
        }
        if (field.children) {
          extractPaths(field.children)
        }
      }
    }

    extractPaths(availableVariables.value.trigger)

    for (const node of availableVariables.value.nodes) {
      extractPaths(node.fields)
    }

    extractPaths(availableVariables.value.vars)
    extractPaths(availableVariables.value.config)

    return paths
  })

  const searchVariables = (query: string): VariableField[] => {
    const results: VariableField[] = []
    const lowerQuery = query.toLowerCase()

    const searchFields = (fields: VariableField[]) => {
      for (const field of fields) {
        if (field.path.toLowerCase().includes(lowerQuery)) {
          results.push(field)
        }
        if (field.children) {
          searchFields(field.children)
        }
      }
    }

    searchFields(availableVariables.value.trigger)

    for (const node of availableVariables.value.nodes) {
      searchFields(node.fields)
    }

    searchFields(availableVariables.value.vars)
    searchFields(availableVariables.value.config)

    return results
  }

  watch(
    () => [currentNodeId.value, executionStore.nodeResultsVersion],
    () => {
      loadVariablesFromExecution()
    },
    { deep: true, immediate: true }
  )

  return {
    availableVariables,
    getAllVariablePaths,
    generateVariablePath,
    searchVariables,
    loadVariablesFromExecution
  }
}
