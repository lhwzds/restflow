import { ref, type Ref } from 'vue'
import { useExecutionStore } from '../../stores/executionStore'

export function useExecutionPanelResize(panelRef: Ref<HTMLElement | undefined>) {
  const executionStore = useExecutionStore()
  
  const isResizing = ref(false)
  const startY = ref(0)
  const startHeight = ref(0)

  const startResize = (event: MouseEvent) => {
    isResizing.value = true
    startY.value = event.clientY
    const panel = panelRef.value
    if (panel) {
      startHeight.value = panel.offsetHeight
    }
    
    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', stopResize)
    document.body.style.userSelect = 'none'
    document.body.style.cursor = 'ns-resize'
  }

  const handleMouseMove = (event: MouseEvent) => {
    if (!isResizing.value) return
    
    const deltaY = startY.value - event.clientY
    const newHeight = startHeight.value + deltaY
    const viewportHeight = window.innerHeight
    const percentage = (newHeight / viewportHeight) * 100
    
    executionStore.setPanelHeight(percentage)
  }

  const stopResize = () => {
    isResizing.value = false
    document.removeEventListener('mousemove', handleMouseMove)
    document.removeEventListener('mouseup', stopResize)
    document.body.style.userSelect = ''
    document.body.style.cursor = ''
  }

  return {
    isResizing,
    startResize,
    stopResize
  }
}