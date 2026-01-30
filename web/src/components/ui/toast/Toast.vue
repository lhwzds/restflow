<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { ToastRoot, type ToastRootEmits, type ToastRootProps, useForwardPropsEmits } from 'radix-vue'
import { cn } from '@/lib/utils'
import { toastVariants, type ToastVariants } from './toast'

const props = defineProps<ToastRootProps & { class?: HTMLAttributes['class']; variant?: ToastVariants['variant'] }>()
const emits = defineEmits<ToastRootEmits>()

const delegatedProps = computed(() => {
  const { class: _, variant: _v, ...delegated } = props
  return delegated
})

const forwarded = useForwardPropsEmits(delegatedProps, emits)
</script>

<template>
  <ToastRoot v-bind="forwarded" :class="cn(toastVariants({ variant }), props.class)">
    <slot />
  </ToastRoot>
</template>
