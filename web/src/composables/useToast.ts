import { toast } from 'vue-sonner'

/**
 * Toast notification composable using vue-sonner
 * Replaces Element Plus ElMessage
 */
export function useToast() {
  return {
    /**
     * Show a success toast message
     */
    success: (message: string) => {
      toast.success(message)
    },

    /**
     * Show an error toast message
     */
    error: (message: string) => {
      toast.error(message)
    },

    /**
     * Show a warning toast message
     */
    warning: (message: string) => {
      toast.warning(message)
    },

    /**
     * Show an info toast message
     */
    info: (message: string) => {
      toast.info(message)
    },

    /**
     * Show a loading toast that can be updated
     */
    loading: (message: string) => {
      return toast.loading(message)
    },

    /**
     * Dismiss a specific toast by id
     */
    dismiss: (id?: string | number) => {
      toast.dismiss(id)
    },
  }
}

// Direct exports for simple usage
export const showSuccess = (message: string) => toast.success(message)
export const showError = (message: string) => toast.error(message)
export const showWarning = (message: string) => toast.warning(message)
export const showInfo = (message: string) => toast.info(message)
