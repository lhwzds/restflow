import { ref, onMounted, onBeforeUnmount } from 'vue'

const STORAGE_KEY = 'agent-panel-width'
const MIN_WIDTH = 300
const MAX_WIDTH = 800
const DEFAULT_WIDTH = 450

export function useAgentPanelResize() {
  const panelWidth = ref(DEFAULT_WIDTH)
  const isDragging = ref(false)
  const startX = ref(0)
  const startWidth = ref(0)

  // Load width from localStorage
  function loadSavedWidth() {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved) {
      const width = parseInt(saved, 10)
      if (width >= MIN_WIDTH && width <= MAX_WIDTH) {
        panelWidth.value = width
      }
    }
  }

  // Save width to localStorage
  function saveWidth() {
    localStorage.setItem(STORAGE_KEY, panelWidth.value.toString())
  }

  // Start dragging
  function startDragging(event: MouseEvent) {
    isDragging.value = true
    startX.value = event.clientX
    startWidth.value = panelWidth.value

    // Add global event listeners
    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', stopDragging)

    // Prevent text selection
    document.body.style.userSelect = 'none'
    document.body.style.cursor = 'ew-resize'
  }

  // Handle dragging
  function handleMouseMove(event: MouseEvent) {
    if (!isDragging.value) return

    const deltaX = event.clientX - startX.value
    const newWidth = startWidth.value + deltaX

    // Limit width range
    if (newWidth >= MIN_WIDTH && newWidth <= MAX_WIDTH) {
      panelWidth.value = newWidth
    }
  }

  // Stop dragging
  function stopDragging() {
    if (!isDragging.value) return

    isDragging.value = false

    // Remove global event listeners
    document.removeEventListener('mousemove', handleMouseMove)
    document.removeEventListener('mouseup', stopDragging)

    // Restore text selection
    document.body.style.userSelect = ''
    document.body.style.cursor = ''

    // Save width
    saveWidth()
  }

  // Reset width
  function resetWidth() {
    panelWidth.value = DEFAULT_WIDTH
    saveWidth()
  }

  // Load saved width when component mounts
  onMounted(() => {
    loadSavedWidth()
  })

  // Cleanup when component unmounts
  onBeforeUnmount(() => {
    if (isDragging.value) {
      stopDragging()
    }
  })

  return {
    panelWidth,
    isDragging,
    startDragging,
    resetWidth,
    MIN_WIDTH,
    MAX_WIDTH,
    DEFAULT_WIDTH,
  }
}
