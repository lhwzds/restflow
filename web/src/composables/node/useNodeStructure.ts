import { computed, ref } from 'vue'
import { useNodeExecutionStatus } from './useNodeExecutionStatus'
import { useNodeInfoPopup } from './useNodeInfoPopup'

/**
 * Unified node structure composable
 * Integrates node execution status, info popup, and action buttons
 *
 * @param nodeId - Node ID
 * @returns All required node states and methods
 */
export function useNodeStructure(nodeId: string) {
  const executionStatus = useNodeExecutionStatus()
  const statusClass = computed(() => executionStatus.getNodeStatusClass(nodeId))
  const executionTime = computed(() => {
    const time = executionStatus.getNodeExecutionTime(nodeId)
    return time ? executionStatus.formatExecutionTime(time) : null
  })

  const infoPopup = useNodeInfoPopup(nodeId)

  const showActions = ref(false)
  const onMouseEnter = () => {
    showActions.value = true
  }
  const onMouseLeave = () => {
    showActions.value = false
  }

  return {
    statusClass,
    executionTime,
    ...infoPopup,
    showActions,
    onMouseEnter,
    onMouseLeave,
  }
}