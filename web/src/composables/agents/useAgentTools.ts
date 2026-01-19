import { ref, readonly, type Ref } from 'vue'
import { listTools, type ToolInfo } from '@/api/tools'

export interface AgentTool {
  value: string
  label: string
  description: string
  parameters?: Record<string, unknown>
}

/**
 * Format tool name to display label
 * e.g., "http_request" -> "HTTP Request"
 */
function formatToolLabel(name: string): string {
  return name
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

/**
 * Convert API ToolInfo to frontend AgentTool format
 */
function toAgentTool(tool: ToolInfo): AgentTool {
  return {
    value: tool.name,
    label: formatToolLabel(tool.name),
    description: tool.description,
    parameters: tool.parameters,
  }
}

/**
 * Agent tools management composable
 * Operates directly on the provided tools ref (single source of truth)
 */
export function useAgentTools(tools: Ref<string[]>) {
  const availableTools = ref<AgentTool[]>([])
  const isLoading = ref(false)
  const error = ref<string | null>(null)
  const selectedToolValue = ref('')

  /**
   * Load tools from backend API
   */
  const loadTools = async () => {
    if (isLoading.value) return

    isLoading.value = true
    error.value = null

    try {
      const toolList = await listTools()
      availableTools.value = toolList.map(toAgentTool)
    } catch (e: unknown) {
      const errorMessage = e instanceof Error ? e.message : 'Failed to load tools'
      error.value = errorMessage
      console.error('Failed to load tools:', e)
      availableTools.value = []
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Add a tool to the selected list
   */
  const addTool = () => {
    if (selectedToolValue.value && !tools.value.includes(selectedToolValue.value)) {
      tools.value.push(selectedToolValue.value)
      selectedToolValue.value = ''
    }
  }

  /**
   * Remove a tool from the selected list
   */
  const removeTool = (toolValue: string) => {
    const index = tools.value.indexOf(toolValue)
    if (index > -1) {
      tools.value.splice(index, 1)
    }
  }

  /**
   * Get tool display label
   */
  const getToolLabel = (value: string): string => {
    const tool = availableTools.value.find((t) => t.value === value)
    return tool?.label || formatToolLabel(value)
  }

  /**
   * Get tool description
   */
  const getToolDescription = (value: string): string => {
    const tool = availableTools.value.find((t) => t.value === value)
    return tool?.description || ''
  }

  /**
   * Get available tools for dropdown options (not yet selected)
   */
  const getAvailableTools = (): AgentTool[] => {
    return availableTools.value.filter((tool) => !tools.value.includes(tool.value))
  }

  return {
    // State
    availableTools: readonly(availableTools),
    isLoading: readonly(isLoading),
    error: readonly(error),
    selectedToolValue,

    // Actions
    loadTools,
    addTool,
    removeTool,
    getToolLabel,
    getToolDescription,
    getAvailableTools,
  }
}
