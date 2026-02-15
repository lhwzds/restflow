import { ref } from 'vue'

export interface ConfirmOptions {
  title: string
  description: string
  confirmText?: string
  cancelText?: string
  variant?: 'default' | 'destructive'
}

// Global state for the confirm dialog
const isOpen = ref(false)
const options = ref<ConfirmOptions>({
  title: '',
  description: '',
  confirmText: 'Confirm',
  cancelText: 'Cancel',
  variant: 'default',
})
let resolvePromise: ((value: boolean) => void) | null = null

/**
 * Confirm dialog composable
 * Replaces Element Plus ElMessageBox.confirm
 */
export function useConfirm() {
  function confirm(opts: ConfirmOptions): Promise<boolean> {
    options.value = {
      ...opts,
      confirmText: opts.confirmText ?? 'Confirm',
      cancelText: opts.cancelText ?? 'Cancel',
      variant: opts.variant ?? 'default',
    }
    isOpen.value = true

    return new Promise((resolve) => {
      resolvePromise = resolve
    })
  }

  function handleConfirm() {
    isOpen.value = false
    resolvePromise?.(true)
    resolvePromise = null
  }

  function handleCancel() {
    isOpen.value = false
    resolvePromise?.(false)
    resolvePromise = null
  }

  return {
    // State
    isOpen,
    options,
    // Actions
    confirm,
    handleConfirm,
    handleCancel,
  }
}

/**
 * Convenience function for delete confirmations
 */
export async function confirmDelete(itemName: string, itemType = 'item'): Promise<boolean> {
  const { confirm } = useConfirm()
  return confirm({
    title: `Delete ${itemType}`,
    description: `Are you sure you want to delete "${itemName}"? This action cannot be undone.`,
    confirmText: 'Delete',
    cancelText: 'Cancel',
    variant: 'destructive',
  })
}

/**
 * Convenience function for unsaved changes confirmation
 */
export async function confirmUnsavedChanges(): Promise<boolean> {
  const { confirm } = useConfirm()
  return confirm({
    title: 'Unsaved Changes',
    description: 'You have unsaved changes. Are you sure you want to leave?',
    confirmText: 'Leave',
    cancelText: 'Stay',
    variant: 'destructive',
  })
}
