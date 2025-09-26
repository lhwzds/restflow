import { ref, readonly } from 'vue'

export interface AgentTool {
  value: string
  label: string
  description: string
}

/**
 * Agent tools management composable
 * Provides unified tool list and management logic
 */
export function useAgentTools(initialTools: string[] = []) {
  const AVAILABLE_TOOLS: readonly AgentTool[] = readonly([
    {
      value: 'add',
      label: 'Addition Calculator',
      description: 'Adds two numbers together'
    },
    {
      value: 'get_current_time',
      label: 'Get Current Time',
      description: 'Returns the current system time'
    }
  ])

  const selectedTools = ref<string[]>([...initialTools])

  const selectedToolValue = ref('')

  /**
   * Add a tool to the selected list
   */
  const addTool = () => {
    if (selectedToolValue.value && !selectedTools.value.includes(selectedToolValue.value)) {
      selectedTools.value.push(selectedToolValue.value)
      selectedToolValue.value = ''
    }
  }

  /**
   * Remove a tool from the selected list
   */
  const removeTool = (toolValue: string) => {
    const index = selectedTools.value.indexOf(toolValue)
    if (index > -1) {
      selectedTools.value.splice(index, 1)
    }
  }

  /**
   * Get tool display label
   */
  const getToolLabel = (value: string): string => {
    const tool = AVAILABLE_TOOLS.find(t => t.value === value)
    return tool?.label || value
  }

  /**
   * Get tool description
   */
  const getToolDescription = (value: string): string => {
    const tool = AVAILABLE_TOOLS.find(t => t.value === value)
    return tool?.description || ''
  }

  /**
   * Get available tools for dropdown options
   */
  const getAvailableTools = (): AgentTool[] => {
    return AVAILABLE_TOOLS.filter(tool =>
      !selectedTools.value.includes(tool.value)
    ) as AgentTool[]
  }

  /**
   * Reset tool selection
   */
  const resetTools = (tools: string[] = []) => {
    selectedTools.value = [...tools]
    selectedToolValue.value = ''
  }

  return {
    AVAILABLE_TOOLS,
    selectedTools,
    selectedToolValue,
    addTool,
    removeTool,
    getToolLabel,
    getToolDescription,
    getAvailableTools,
    resetTools
  }
}