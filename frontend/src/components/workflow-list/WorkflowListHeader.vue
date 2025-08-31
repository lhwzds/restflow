<script setup lang="ts">
import { Plus, Search } from '@element-plus/icons-vue'
import { ElButton, ElInput } from 'element-plus'

const searchQuery = defineModel<string>('searchQuery', { default: '' })

const emit = defineEmits<{
  search: []
  create: []
  clear: []
}>()

function handleSearch() {
  emit('search')
}

function handleClear() {
  searchQuery.value = ''
  emit('clear')
}
</script>

<template>
  <div class="workflow-list-header">
    <ElInput
      v-model="searchQuery"
      placeholder="Search workflows..."
      :prefix-icon="Search"
      clearable
      class="search-input"
      @clear="handleClear"
      @keyup.enter="handleSearch"
    />
    <ElButton type="primary" :icon="Plus" @click="emit('create')">
      New Workflow
    </ElButton>
  </div>
</template>

<style lang="scss" scoped>
.workflow-list-header {
  display: flex;
  align-items: center;
  gap: 16px;
}

.search-input {
  width: 300px;
}
</style>