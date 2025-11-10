<script setup lang="ts">
import { ElButton, ElEmpty } from 'element-plus'

interface Props {
  searchQuery?: string
  isLoading?: boolean
}

const props = defineProps<Props>()

const emit = defineEmits<{
  createWorkflow: []
  clearSearch: []
}>()

const description = props.searchQuery
  ? 'No workflows found matching your search'
  : 'No workflows yet'

const buttonText = props.searchQuery ? 'Clear search' : 'Create your first workflow'

function handleAction() {
  if (props.searchQuery) {
    emit('clearSearch')
  } else {
    emit('createWorkflow')
  }
}
</script>

<template>
  <div v-if="!isLoading" class="empty-state">
    <ElEmpty :description="description">
      <ElButton :type="searchQuery ? 'default' : 'primary'" @click="handleAction">
        {{ buttonText }}
      </ElButton>
    </ElEmpty>
  </div>
</template>

<style lang="scss" scoped>
.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 60vh;
}
</style>
