<script setup lang="ts">
import { Toaster as Sonner, type ToasterProps } from 'vue-sonner'

const props = withDefaults(defineProps<ToasterProps>(), {
  position: 'top-center',
  richColors: true,
})
</script>

<template>
  <Sonner v-bind="props" />
</template>
