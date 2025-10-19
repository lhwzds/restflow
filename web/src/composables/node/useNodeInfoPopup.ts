import { ref, nextTick } from 'vue'
import type { PopupType } from '@/components/nodes/NodeInfoPopup.vue'
import { useExecutionStore } from '@/stores/executionStore'

/**
 * Composable for node info popup state management
 *
 * Creates popup state for a node. Should be called once in BaseNode
 * and shared with child components via provide/inject.
 *
 * State lifecycle is tied to the node component - automatically cleaned
 * up when the node unmounts.
 */
export function useNodeInfoPopup(nodeId: string) {
  const executionStore = useExecutionStore()

  const popupVisible = ref(false)
  const popupType = ref<PopupType>('time')
  const popupPosition = ref({ x: 0, y: 0 })
  const activeTab = ref<PopupType | null>(null)

  const nodeResult = () => executionStore.nodeResults.get(nodeId) || null

  const hasInput = () => {
    const result = nodeResult()
    return result?.input !== undefined && result.input !== null
  }

  const hasOutput = () => {
    const result = nodeResult()
    return result?.output !== undefined && result.output !== null
  }

  const hasExecutionTime = () => {
    const result = nodeResult()
    return result?.executionTime !== undefined
  }

  const calculatePosition = (event: MouseEvent) => {
    const target = event.currentTarget as HTMLElement
    const rect = target.getBoundingClientRect()

    return {
      x: rect.left + rect.width / 2 - 100,
      y: rect.bottom + 4
    }
  }

  const showPopup = async (event: MouseEvent, type: PopupType) => {
    if (popupVisible.value && popupType.value === type) {
      popupVisible.value = false
      activeTab.value = null
      return
    }

    popupType.value = type
    popupPosition.value = calculatePosition(event)
    popupVisible.value = false

    await nextTick()
    popupVisible.value = true
    activeTab.value = type
  }

  const showTimePopup = (event: MouseEvent) => {
    if (!hasExecutionTime()) return
    showPopup(event, 'time')
  }

  const showInputPopup = (event: MouseEvent) => {
    if (!hasInput()) return
    showPopup(event, 'input')
  }

  const showOutputPopup = (event: MouseEvent) => {
    if (!hasOutput()) return
    showPopup(event, 'output')
  }

  const closePopup = () => {
    popupVisible.value = false
    activeTab.value = null
  }

  return {
    popupVisible,
    popupType,
    popupPosition,
    nodeResult,
    activeTab,
    hasInput,
    hasOutput,
    hasExecutionTime,
    showTimePopup,
    showInputPopup,
    showOutputPopup,
    closePopup
  }
}