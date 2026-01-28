<script setup lang="ts">
import { PopoverTrigger } from 'radix-vue'

defineProps<{
  asChild?: boolean
}>()
</script>

<template>
  <PopoverTrigger :as-child="asChild">
    <slot />
  </PopoverTrigger>
</template>
