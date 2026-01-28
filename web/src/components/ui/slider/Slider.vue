<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import {
  SliderRange,
  SliderRoot,
  type SliderRootEmits,
  type SliderRootProps,
  SliderThumb,
  SliderTrack,
} from 'radix-vue'
import { cn } from '@/lib/utils'

const props = defineProps<SliderRootProps & { class?: HTMLAttributes['class'] }>()

const emits = defineEmits<SliderRootEmits>()
</script>

<template>
  <SliderRoot
    :class="cn('relative flex w-full touch-none select-none items-center', props.class)"
    v-bind="props"
    @update:model-value="(v) => emits('update:modelValue', v)"
  >
    <SliderTrack class="relative h-1.5 w-full grow overflow-hidden rounded-full bg-primary/20">
      <SliderRange class="absolute h-full bg-primary" />
    </SliderTrack>
    <SliderThumb
      v-for="(_, key) in modelValue"
      :key="key"
      class="block h-4 w-4 rounded-full border border-primary/50 bg-background shadow transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
    />
  </SliderRoot>
</template>
