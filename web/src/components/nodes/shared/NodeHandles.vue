<script setup lang="ts">
import { Handle, Position } from '@vue-flow/core'
</script>

<template>
  <Handle
    type="target"
    :position="Position.Left"
    class="custom-handle input-handle"
  />
  <Handle
    type="source"
    :position="Position.Right"
    class="custom-handle output-handle"
  />
</template>

<style lang="scss" scoped>
.custom-handle {
  &.input-handle {
    left: -4px;
  }

  &.output-handle {
    right: -4px;
  }
}
</style>
