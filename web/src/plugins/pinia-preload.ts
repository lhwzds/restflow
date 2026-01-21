import type { PiniaPluginContext } from 'pinia'
import { ElMessage } from 'element-plus'

/**
 * Pinia plugin to preload critical data when stores are initialized
 * This ensures data is loaded as early as possible without blocking app startup
 */
export function preloadPlugin({ store }: PiniaPluginContext) {
  // Only run once when models store is initialized
  if (store.$id === 'models') {
    // Start loading immediately (non-blocking)
    // The loadModels action will handle the actual API call
    store.loadModels().catch((error: Error) => {
      console.error('[Preload] Failed to load AI models:', error)

      // Show user-friendly notification
      ElMessage.error({
        message: 'Failed to load AI models. Some features may be unavailable.',
        duration: 5000,
        showClose: true,
      })
    })
  }
}
