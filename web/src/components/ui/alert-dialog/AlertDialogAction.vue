<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { AlertDialogAction, type AlertDialogActionProps } from 'radix-vue'
import { cn } from '@/lib/utils'
import { buttonVariants } from '@/components/ui/button'

defineOptions({
  inheritAttrs: false,
})

const props = defineProps<AlertDialogActionProps & { class?: HTMLAttributes['class'] }>()
</script>

<template>
  <AlertDialogAction :class="cn(buttonVariants(), props.class)" v-bind="{ ...props, ...$attrs }">
    <slot />
  </AlertDialogAction>
</template>
