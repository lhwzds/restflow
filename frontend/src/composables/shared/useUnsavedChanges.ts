import { onBeforeUnmount, computed } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { useWorkflowStore } from '../../stores/workflowStore'

export function useUnsavedChanges() {
  const workflowStore = useWorkflowStore()
  
  const hasChanges = computed(() => workflowStore.hasUnsavedChanges)

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
          workflowStore.markAsSaved()
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
    markAsDirty: workflowStore.markAsDirty,
    markAsSaved: workflowStore.markAsSaved,
  }
}
