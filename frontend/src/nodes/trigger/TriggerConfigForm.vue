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
  padding: 12px;
  background-color: #f1f5f9;
  border-radius: 6px;
  color: #475569;
  font-size: 14px;
}
</style>