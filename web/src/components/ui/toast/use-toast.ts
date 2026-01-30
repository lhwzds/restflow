import { ref, computed } from 'vue'

const TOAST_LIMIT = 5
const TOAST_REMOVE_DELAY = 1000000

export type ToastVariant = 'default' | 'destructive'

export interface ToastProps {
  id?: string
  title?: string
  description?: string
  action?: {
    label: string
    onClick: () => void
  }
  variant?: ToastVariant
  duration?: number
}

export interface Toast extends ToastProps {
  id: string
  open: boolean
}

const toasts = ref<Toast[]>([])

let toastCount = 0

function generateId() {
  toastCount = (toastCount + 1) % Number.MAX_VALUE
  return toastCount.toString()
}

function addToast(props: ToastProps) {
  const id = props.id || generateId()

  const toast: Toast = {
    ...props,
    id,
    open: true,
  }

  toasts.value = [toast, ...toasts.value].slice(0, TOAST_LIMIT)

  return id
}

function updateToast(id: string, props: Partial<ToastProps>) {
  toasts.value = toasts.value.map((t) =>
    t.id === id ? { ...t, ...props } : t
  )
}

function dismissToast(id?: string) {
  if (id) {
    toasts.value = toasts.value.map((t) =>
      t.id === id ? { ...t, open: false } : t
    )
  } else {
    toasts.value = toasts.value.map((t) => ({ ...t, open: false }))
  }

  setTimeout(() => {
    toasts.value = id
      ? toasts.value.filter((t) => t.id !== id)
      : []
  }, TOAST_REMOVE_DELAY)
}

export function useToast() {
  return {
    toasts: computed(() => toasts.value),
    toast: (props: ToastProps) => {
      const id = addToast(props)

      return {
        id,
        dismiss: () => dismissToast(id),
        update: (props: Partial<ToastProps>) => updateToast(id, props),
      }
    },
    dismiss: dismissToast,
  }
}

export const toast = (props: ToastProps) => {
  const id = addToast(props)

  return {
    id,
    dismiss: () => dismissToast(id),
    update: (props: Partial<ToastProps>) => updateToast(id, props),
  }
}
