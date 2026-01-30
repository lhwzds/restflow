import { ref, computed, type Component } from 'vue'

const TOAST_LIMIT = 1
const _TOAST_REMOVE_DELAY = 1000000

export type ToasterToast = {
  id: string
  title?: string
  description?: string
  action?: Component
  variant?: 'default' | 'destructive'
}

const toasts = ref<ToasterToast[]>([])

let count = 0

function genId() {
  count = (count + 1) % Number.MAX_VALUE
  return count.toString()
}

function addToast(toast: Omit<ToasterToast, 'id'>) {
  const id = genId()

  const newToast = { ...toast, id }
  toasts.value = [newToast, ...toasts.value].slice(0, TOAST_LIMIT)

  return {
    id,
    dismiss: () => dismissToast(id),
    update: (props: Partial<ToasterToast>) => updateToast(id, props),
  }
}

function updateToast(id: string, props: Partial<ToasterToast>) {
  toasts.value = toasts.value.map((t) =>
    t.id === id ? { ...t, ...props } : t,
  )
}

function dismissToast(toastId?: string) {
  if (toastId) {
    toasts.value = toasts.value.filter((t) => t.id !== toastId)
  } else {
    toasts.value = []
  }
}

export function useToast() {
  return {
    toasts: computed(() => toasts.value),
    toast: addToast,
    dismiss: dismissToast,
  }
}

export const toast = addToast
