import { useVueFlow } from '@vue-flow/core'
import { useWorkflowStore } from '../stores/workflowStore'

export function useDragAndDrop() {
  const { project, vueFlowRef } = useVueFlow()
  const workflowStore = useWorkflowStore()
  
  const handleDragOver = (event: DragEvent) => {
    event.preventDefault()
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'move'
    }
  }
  
  const handleDrop = (event: DragEvent) => {
    event.preventDefault()
    
    const data = event.dataTransfer?.getData('application/vueflow')
    if (!data) return
    
    const template = JSON.parse(data)
    const position = project({
      x: event.clientX - vueFlowRef.value!.getBoundingClientRect().left,
      y: event.clientY - vueFlowRef.value!.getBoundingClientRect().top,
    })
    
    workflowStore.createNode(template, position)
  }
  
  return {
    handleDragOver,
    handleDrop,
  }
}