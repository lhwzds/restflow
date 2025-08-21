import { onBeforeUnmount } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { computed } from 'vue'
import { useWorkflowStore } from '../../stores/workflowStore'

export function useUnsavedChanges() {
  const workflowStore = useWorkflowStore()
  
  // Use store's state
  const hasChanges = computed(() => workflowStore.hasUnsavedChanges)
  
  // Delegate to store
  const markAsDirty = () => workflowStore.markAsDirty()
  const markAsSaved = () => workflowStore.markAsSaved()

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
    onBeforeRouteLeave((to, from, next) => {
      if (hasChanges.value) {
        if (window.confirm('You have unsaved changes. Are you sure you want to leave?')) {
          markAsSaved() // Reset state when user confirms leaving
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

  return {
    hasChanges,
    markAsDirty,
    markAsSaved,
  }
}
