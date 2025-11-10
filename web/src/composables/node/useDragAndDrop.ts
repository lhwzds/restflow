import { useVueFlow } from '@vue-flow/core'
import { useNodeOperations } from './useNodeOperations'

export function useDragAndDrop() {
  const { project, vueFlowRef } = useVueFlow()
  const { createNode } = useNodeOperations()

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

    createNode(template, position)
  }

  return {
    handleDragOver,
    handleDrop,
  }
}
