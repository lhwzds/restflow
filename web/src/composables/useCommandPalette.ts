import { ref, readonly } from 'vue'

// Module-level singleton — same pattern as useConfirm.ts
const isOpen = ref(false)

export function useCommandPalette() {
  return {
    isOpen: readonly(isOpen),
    open: () => {
      isOpen.value = true
    },
    close: () => {
      isOpen.value = false
    },
  }
}
