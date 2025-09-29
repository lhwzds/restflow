import { ref, nextTick } from 'vue'
import type { PopupType } from '@/components/nodes/NodeInfoPopup.vue'
import { useExecutionStore } from '@/stores/executionStore'

export function useNodeInfoPopup(nodeId: string) {
  const executionStore = useExecutionStore()

  // Popup state
  const popupVisible = ref(false)
  const popupType = ref<PopupType>('time')
  const popupPosition = ref({ x: 0, y: 0 })

  // Active tab (for highlighting)
  const activeTab = ref<PopupType | null>(null)

  // Get node execution result
  const nodeResult = () => executionStore.nodeResults.get(nodeId) || null

  // Check if there is data
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

  // Calculate popup position
  const calculatePosition = (event: MouseEvent) => {
    const target = event.currentTarget as HTMLElement
    const rect = target.getBoundingClientRect()

    // Popup is placed directly below the clicked tag
    // Horizontal position is centered on the tag
    return {
      x: rect.left + rect.width / 2 - 100, // Assuming popup width is 200px, center aligned
      y: rect.bottom + 4 // Directly below the tag, with 4px gap
    }
  }

  // Show popup
  const showPopup = async (event: MouseEvent, type: PopupType) => {
    // If the same type of popup is already visible, close it
    if (popupVisible.value && popupType.value === type) {
      popupVisible.value = false
      activeTab.value = null
      return
    }

    popupType.value = type
    popupPosition.value = calculatePosition(event)
    popupVisible.value = false

    // Wait for DOM update before showing to ensure correct position
    await nextTick()
    popupVisible.value = true
    activeTab.value = type
  }

  // Show time popup
  const showTimePopup = (event: MouseEvent) => {
    if (!hasExecutionTime()) return
    showPopup(event, 'time')
  }

  // Show input popup
  const showInputPopup = (event: MouseEvent) => {
    if (!hasInput()) return
    showPopup(event, 'input')
  }

  // Show output popup
  const showOutputPopup = (event: MouseEvent) => {
    if (!hasOutput()) return
    showPopup(event, 'output')
  }

  // Close popup
  const closePopup = () => {
    popupVisible.value = false
    activeTab.value = null
  }

  return {
    // State
    popupVisible,
    popupType,
    popupPosition,
    nodeResult,
    activeTab,

    // Data checks
    hasInput,
    hasOutput,
    hasExecutionTime,

    // Action methods
    showTimePopup,
    showInputPopup,
    showOutputPopup,
    closePopup
  }
}