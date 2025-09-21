<script setup lang="ts">
interface Props {
  modelValue: any
}

defineProps<Props>()
</script>

<template>
  <div class="trigger-config">
    <div class="info-message">
      This node triggers the workflow manually. No configuration needed.
    </div>
  </div>
</template>

<style lang="scss" scoped>
.info-message {
  padding: var(--rf-spacing-lg);
  background-color: var(--rf-color-bg-secondary);
  border-radius: var(--rf-radius-base);
  color: var(--rf-color-text-regular);
  font-size: var(--rf-font-size-base);
}
</style>