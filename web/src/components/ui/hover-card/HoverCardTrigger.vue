<script setup lang="ts">
import { HoverCardTrigger as RadixHoverCardTrigger } from 'radix-vue'

defineProps<{
  asChild?: boolean
}>()
</script>

<template>
  <RadixHoverCardTrigger :as-child="asChild">
    <slot />
  </RadixHoverCardTrigger>
</template>
