<script setup lang="ts">
import { cn } from '@/lib/utils'
import { ToastRoot } from 'radix-vue'
import { computed, type HTMLAttributes } from 'vue'
import { type VariantProps, cva } from 'class-variance-authority'

export const toastVariants = cva(
  'group pointer-events-auto relative flex w-full items-center justify-between space-x-2 overflow-hidden rounded-md border p-4 pr-6 shadow-lg transition-all data-[swipe=cancel]:translate-x-0 data-[swipe=end]:translate-x-[var(--radix-toast-swipe-end-x)] data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)] data-[swipe=move]:transition-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[swipe=end]:animate-out data-[state=closed]:fade-out-80 data-[state=closed]:slide-out-to-right-full data-[state=open]:slide-in-from-top-full data-[state=open]:sm:slide-in-from-bottom-full',
  {
    variants: {
      variant: {
        default: 'border bg-background text-foreground',
        destructive:
          'destructive group border-destructive bg-destructive text-destructive-foreground',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
)

type ToastVariants = VariantProps<typeof toastVariants>

const props = defineProps<{
  class?: HTMLAttributes['class']
  variant?: ToastVariants['variant']
  open?: boolean
  defaultOpen?: boolean
  type?: 'foreground' | 'background'
  duration?: number
}>()

const emits = defineEmits<{
  (e: 'update:open', value: boolean): void
}>()

const delegatedProps = computed(() => {
  const { class: _, variant: __, ...delegated } = props
  return delegated
})
</script>

<template>
  <ToastRoot
    v-bind="delegatedProps"
    :class="cn(toastVariants({ variant }), props.class)"
    @update:open="emits('update:open', $event)"
  >
    <slot />
  </ToastRoot>
</template>
