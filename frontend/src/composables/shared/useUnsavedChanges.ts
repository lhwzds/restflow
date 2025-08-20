import { onBeforeUnmount, ref } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'

export function useUnsavedChanges() {
  const hasChanges = ref(false)
  
  // Simple state management
  const markAsDirty = () => { hasChanges.value = true }
  const markAsSaved = () => { hasChanges.value = false }
  
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