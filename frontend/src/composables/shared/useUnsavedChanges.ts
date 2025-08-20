import { computed, onBeforeUnmount, ref, watch, type Ref, type WatchSource } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'

export interface UnsavedChangesOptions {
  message?: string
  watchSource?: WatchSource | WatchSource[]
  immediate?: boolean
}

export function useUnsavedChanges(options: UnsavedChangesOptions = {}) {
  const {
    message = 'You have unsaved changes. Are you sure you want to leave?',
    watchSource,
    immediate = false,
  } = options

  const isDirty = ref(immediate)
  const isSaved = ref(!immediate)

  // Track if we should prevent navigation
  const preventNavigation = computed(() => isDirty.value && !isSaved.value)

  /**
   * Mark changes as saved
   */
  const markAsSaved = () => {
    isDirty.value = false
    isSaved.value = true
  }

  /**
   * Mark changes as dirty (unsaved)
   */
  const markAsDirty = () => {
    isDirty.value = true
    isSaved.value = false
  }

  /**
   * Reset the state
   */
  const reset = () => {
    isDirty.value = false
    isSaved.value = true
  }

  /**
   * Set up watchers if watchSource is provided
   */
  if (watchSource) {
    const stopWatcher = watch(
      watchSource,
      () => {
        if (isSaved.value) {
          markAsDirty()
        }
      },
      { deep: true }
    )

    // Clean up watcher on unmount
    onBeforeUnmount(() => {
      stopWatcher()
    })
  }

  /**
   * Handle browser beforeunload event
   */
  const handleBeforeUnload = (event: BeforeUnloadEvent) => {
    if (preventNavigation.value) {
      event.preventDefault()
      event.returnValue = message
      return message
    }
  }

  /**
   * Register/unregister browser navigation prevention
   */
  const registerBrowserPrevention = () => {
    window.addEventListener('beforeunload', handleBeforeUnload)
  }

  const unregisterBrowserPrevention = () => {
    window.removeEventListener('beforeunload', handleBeforeUnload)
  }

  /**
   * Set up Vue Router navigation guard if available
   */
  try {
    onBeforeRouteLeave((to, from, next) => {
      if (preventNavigation.value) {
        const answer = window.confirm(message)
        if (answer) {
          next()
        } else {
          next(false)
        }
      } else {
        next()
      }
    })
  } catch {
    // onBeforeRouteLeave is only available in setup context of a component with router
    // Silently ignore if not available
  }

  /**
   * Enable navigation prevention
   */
  const enable = () => {
    registerBrowserPrevention()
  }

  /**
   * Disable navigation prevention
   */
  const disable = () => {
    unregisterBrowserPrevention()
    reset()
  }

  // Auto-register on mount
  enable()

  // Clean up on unmount
  onBeforeUnmount(() => {
    disable()
  })

  return {
    isDirty: computed(() => isDirty.value),
    isSaved: computed(() => isSaved.value),
    preventNavigation,
    markAsSaved,
    markAsDirty,
    reset,
    enable,
    disable,
  }
}

/**
 * Composable for tracking unsaved changes with a custom confirm dialog
 */
export function useUnsavedChangesWithDialog(
  options: UnsavedChangesOptions & {
    onConfirm?: () => void | Promise<void>
    onCancel?: () => void
  } = {}
) {
  const { onConfirm, onCancel, ...baseOptions } = options
  const base = useUnsavedChanges(baseOptions)
  const showDialog = ref(false)
  const pendingNavigation = ref<() => void>()

  /**
   * Show confirmation dialog
   */
  const confirmNavigation = async (): Promise<boolean> => {
    if (!base.preventNavigation.value) {
      return true
    }

    return new Promise((resolve) => {
      showDialog.value = true
      pendingNavigation.value = () => {
        showDialog.value = false
        resolve(true)
        onConfirm?.()
      }
    })
  }

  /**
   * Handle dialog confirmation
   */
  const handleConfirm = async () => {
    if (pendingNavigation.value) {
      await onConfirm?.()
      pendingNavigation.value()
      pendingNavigation.value = undefined
    }
  }

  /**
   * Handle dialog cancellation
   */
  const handleCancel = () => {
    showDialog.value = false
    pendingNavigation.value = undefined
    onCancel?.()
  }

  return {
    ...base,
    showDialog: computed(() => showDialog.value),
    confirmNavigation,
    handleConfirm,
    handleCancel,
  }
}