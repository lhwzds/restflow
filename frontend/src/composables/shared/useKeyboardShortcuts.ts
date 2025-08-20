import { onMounted, onUnmounted } from 'vue'

export interface ShortcutConfig {
  key: string
  ctrl?: boolean
  meta?: boolean
  shift?: boolean
  alt?: boolean
  handler: () => void
  preventDefault?: boolean
}

export function useKeyboardShortcuts(shortcuts: Record<string, () => void> | ShortcutConfig[]) {
  const normalizedShortcuts: ShortcutConfig[] = []

  // Normalize shortcuts format
  if (Array.isArray(shortcuts)) {
    normalizedShortcuts.push(...shortcuts)
  } else {
    // Convert simple format to ShortcutConfig format
    for (const [combo, handler] of Object.entries(shortcuts)) {
      const parts = combo.toLowerCase().split('+')
      const config: ShortcutConfig = {
        key: parts[parts.length - 1],
        handler,
        preventDefault: true,
      }

      for (const part of parts.slice(0, -1)) {
        if (part === 'ctrl') config.ctrl = true
        if (part === 'cmd' || part === 'meta') config.meta = true
        if (part === 'shift') config.shift = true
        if (part === 'alt') config.alt = true
      }

      normalizedShortcuts.push(config)
    }
  }

  const handleKeyDown = (event: KeyboardEvent) => {
    for (const shortcut of normalizedShortcuts) {
      const ctrlOrMeta = shortcut.ctrl || shortcut.meta
      const isCtrlPressed = event.ctrlKey || event.metaKey

      if (
        shortcut.key === event.key.toLowerCase() &&
        (!ctrlOrMeta || isCtrlPressed === true) &&
        (!shortcut.shift || event.shiftKey === shortcut.shift) &&
        (!shortcut.alt || event.altKey === shortcut.alt)
      ) {
        if (shortcut.preventDefault !== false) {
          event.preventDefault()
        }
        shortcut.handler()
        break
      }
    }
  }

  const register = () => {
    document.addEventListener('keydown', handleKeyDown)
  }

  const unregister = () => {
    document.removeEventListener('keydown', handleKeyDown)
  }

  // Auto register/unregister with component lifecycle
  onMounted(register)
  onUnmounted(unregister)

  return {
    register,
    unregister,
  }
}