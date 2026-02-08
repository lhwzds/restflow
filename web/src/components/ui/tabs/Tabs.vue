<script setup lang="ts">
import { TabsRoot } from 'radix-vue'

const props = defineProps<{
  defaultValue?: string
  modelValue?: string
  orientation?: 'horizontal' | 'vertical'
  dir?: 'ltr' | 'rtl'
  activationMode?: 'automatic' | 'manual'
}>()

const emits = defineEmits<{
  (e: 'update:modelValue', value: string): void
}>()
</script>

<template>
  <TabsRoot v-bind="props" @update:model-value="emits('update:modelValue', $event)">
    <slot />
  </TabsRoot>
</template>
