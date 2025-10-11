import { reactive } from 'vue'

export interface ContextMenuState {
  show: boolean
  x: number
  y: number
  nodeId: string | null
}

export function useContextMenu() {
  const state = reactive<ContextMenuState>({
    show: false,
    x: 0,
    y: 0,
    nodeId: null,
  })

  const show = (event: MouseEvent | TouchEvent, nodeId: string | null = null) => {
    event.preventDefault()
    const x = 'clientX' in event ? event.clientX : (event as TouchEvent).touches[0]?.clientX || 0
    const y = 'clientY' in event ? event.clientY : (event as TouchEvent).touches[0]?.clientY || 0
    
    Object.assign(state, {
      show: true,
      x,
      y,
      nodeId,
    })
  }

  const hide = () => {
    state.show = false
  }

  return {
    state,
    show,
    hide,
  }
}