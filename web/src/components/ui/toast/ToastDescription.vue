<script setup lang="ts">
import { cn } from '@/lib/utils'
import { ToastDescription, type ToastDescriptionProps } from 'radix-vue'
import { computed, type HTMLAttributes } from 'vue'

const props = defineProps<ToastDescriptionProps & { class?: HTMLAttributes['class'] }>()

const delegatedProps = computed(() => {
  const { class: _, ...delegated } = props
  return delegated
})
</script>

<template>
  <ToastDescription v-bind="delegatedProps" :class="cn('text-sm opacity-90', props.class)">
    <slot />
  </ToastDescription>
</template>
