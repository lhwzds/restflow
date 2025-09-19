<script setup lang="ts">
import { ElButton } from 'element-plus'

interface Props {
  count: number
  searchQuery: string
  itemName?: string
}

const props = withDefaults(defineProps<Props>(), {
  itemName: 'item'
})

const emit = defineEmits<{
  clear: []
}>()

function handleClear() {
  emit('clear')
}
</script>

<template>
  <div v-if="searchQuery" class="search-info">
    <span class="search-info__text">
      Found {{ count }} {{ itemName }}{{ count !== 1 ? 's' : '' }} matching "{{ searchQuery }}"
    </span>
    <ElButton link @click="handleClear">Clear</ElButton>
  </div>
</template>

<style lang="scss" scoped>
.search-info {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-md) var(--rf-spacing-lg);
  margin-top: var(--rf-spacing-lg);
  background: var(--rf-color-info-lighter);
  border-radius: var(--rf-radius-base);

  &__text {
    color: var(--rf-color-text-regular);
    font-size: var(--rf-font-size-sm);
  }
}

html.dark {
  .search-info {
    background: var(--rf-color-bg-container);
    border: 1px solid var(--rf-color-border-lighter);
  }
}
</style>