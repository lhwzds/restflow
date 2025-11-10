import { ref, computed } from 'vue'

export type DragType = 'variable' | 'field' | 'node'

export interface DragData {
  type: DragType
  data: string
  metadata?: Record<string, any>
}

/**
 * Global drag and drop state management
 */
const isDragging = ref(false)
const dragData = ref<DragData | null>(null)
const dragTarget = ref<HTMLElement | null>(null)

export function useDragAndDrop() {
  /**
   * Start dragging with data
   */
  const startDrag = (type: DragType, data: string, metadata?: Record<string, any>) => {
    isDragging.value = true
    dragData.value = { type, data, metadata }
  }

  /**
   * End dragging and clear data
   */
  const endDrag = () => {
    isDragging.value = false
    dragData.value = null
    dragTarget.value = null
  }

  /**
   * Set the current drag target element
   */
  const setDragTarget = (element: HTMLElement | null) => {
    dragTarget.value = element
  }

  /**
   * Check if currently dragging a specific type
   */
  const isDraggingType = (type: DragType) => {
    return computed(() => isDragging.value && dragData.value?.type === type)
  }

  /**
   * Get current drag data
   */
  const getCurrentDragData = computed(() => dragData.value)

  /**
   * Check if a drop target should be active
   */
  const isDropTarget = (element: HTMLElement | null) => {
    return computed(() => isDragging.value && element && dragTarget.value === element)
  }

  return {
    isDragging: computed(() => isDragging.value),
    dragData: getCurrentDragData,
    dragTarget: computed(() => dragTarget.value),
    startDrag,
    endDrag,
    setDragTarget,
    isDraggingType,
    isDropTarget,
  }
}
