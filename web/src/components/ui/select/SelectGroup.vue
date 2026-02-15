<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { SelectGroup, type SelectGroupProps } from 'radix-vue'
import { cn } from '@/lib/utils'

const props = defineProps<SelectGroupProps & { class?: HTMLAttributes['class'] }>()
</script>

<template>
  <SelectGroup v-bind="props" :class="cn('p-1 w-full', props.class)">
    <slot />
  </SelectGroup>
</template>
