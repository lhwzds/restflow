<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'

const props = defineProps<NodeProps>()

const x = computed(() => `${Math.round(props.position.x)}px`)
const y = computed(() => `${Math.round(props.position.y)}px`)
</script>

<template>
  <div class="vue-flow__node-default">
    <Handle type="target" :position="Position.Top" />

    <div>{{ data.label }}</div>
    <div>{{ x }} {{ y }}</div>

    <Handle type="source" :position="Position.Bottom" />
  </div>
</template>
