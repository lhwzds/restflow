import { nextTick, ref } from 'vue'

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

type PendingConfirm = {
  options: ConfirmOptions
  resolve: (value: boolean) => void
}

const pendingQueue: PendingConfirm[] = []
let activeConfirm: PendingConfirm | null = null
let isSettling = false

function normalizeOptions(opts: ConfirmOptions): ConfirmOptions {
  return {
    ...opts,
    confirmText: opts.confirmText ?? 'Confirm',
    cancelText: opts.cancelText ?? 'Cancel',
    variant: opts.variant ?? 'default',
  }
}

function showNextConfirm() {
  if (activeConfirm || isSettling) {
    return
  }

  const next = pendingQueue.shift() ?? null
  if (!next) {
    return
  }

  activeConfirm = next
  options.value = next.options
  isOpen.value = true
}

/**
 * Confirm dialog composable
 * Replaces Element Plus ElMessageBox.confirm
 */
export function useConfirm() {
  function confirm(opts: ConfirmOptions): Promise<boolean> {
    return new Promise((resolve) => {
      pendingQueue.push({
        options: normalizeOptions(opts),
        resolve,
      })
      showNextConfirm()
    })
  }

  async function settleConfirmation(value: boolean) {
    const current = activeConfirm
    if (!current) {
      return
    }

    isSettling = true
    isOpen.value = false
    await nextTick()
    activeConfirm = null
    isSettling = false
    current.resolve(value)
    showNextConfirm()
  }

  function handleConfirm() {
    void settleConfirmation(true)
  }

  function handleCancel() {
    void settleConfirmation(false)
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
