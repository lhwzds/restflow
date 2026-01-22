import { onBeforeUnmount, ref, computed, type Ref } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'

/**
 * Composable for handling unsaved changes warnings.
 * Can be used with any component that tracks dirty state.
 */
export function useUnsavedChanges(isDirty?: Ref<boolean>) {
  const internalDirty = ref(false)
  const hasChanges = computed(() => isDirty?.value ?? internalDirty.value)

  // Browser navigation prevention
  const handleBeforeUnload = (e: BeforeUnloadEvent) => {
    if (hasChanges.value) {
      e.preventDefault()
      e.returnValue = ''
      return ''
    }
  }

  // Register browser event
  window.addEventListener('beforeunload', handleBeforeUnload)

  // Vue Router navigation guard
  try {
    onBeforeRouteLeave((_to, _from, next) => {
      if (hasChanges.value) {
        if (window.confirm('You have unsaved changes. Are you sure you want to leave?')) {
          internalDirty.value = false
          next()
        } else {
          next(false)
        }
      } else {
        next()
      }
    })
  } catch {
    // Not in a route component, ignore
  }

  // Cleanup
  onBeforeUnmount(() => {
    window.removeEventListener('beforeunload', handleBeforeUnload)
  })

  function markAsDirty() {
    internalDirty.value = true
  }

  function markAsSaved() {
    internalDirty.value = false
  }

  return {
    hasChanges,
    markAsDirty,
    markAsSaved,
  }
}
