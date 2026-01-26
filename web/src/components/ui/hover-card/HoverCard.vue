<script setup lang="ts">
import { HoverCardRoot } from 'radix-vue'

const props = withDefaults(
  defineProps<{
    openDelay?: number
    closeDelay?: number
  }>(),
  {
    openDelay: 300,
    closeDelay: 100,
  },
)
</script>

<template>
  <HoverCardRoot :open-delay="props.openDelay" :close-delay="props.closeDelay">
    <slot />
  </HoverCardRoot>
</template>
