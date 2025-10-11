<script setup lang="ts">
import { computed } from 'vue'
import { ElButton, ElEmpty } from 'element-plus'

interface Props {
  description?: string
  searchQuery?: string
  actionText?: string
  actionType?: 'primary' | 'default'
  createText?: string
  itemName?: string
  isLoading?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  actionType: 'primary',
  actionText: '',
  createText: 'Create First',
  itemName: 'item'
})

const emit = defineEmits<{
  action: []
  clearSearch: []
}>()

const computedDescription = computed(() => {
  if (props.description) return props.description
  return props.searchQuery
    ? `No ${props.itemName}s found matching your search`
    : `No ${props.itemName}s yet`
})

const computedActionText = computed(() => {
  if (props.actionText) return props.actionText
  return props.searchQuery ? 'Clear search' : `${props.createText} ${props.itemName}`
})

const computedActionType = computed(() => {
  return props.searchQuery ? 'default' : props.actionType
})

function handleAction() {
  if (props.searchQuery) {
    emit('clearSearch')
  } else {
    emit('action')
  }
}
</script>

<template>
  <div v-if="!isLoading" class="empty-state">
    <ElEmpty :description="computedDescription">
      <ElButton
        :type="computedActionType"
        @click="handleAction"
      >
        {{ computedActionText }}
      </ElButton>
    </ElEmpty>
  </div>
</template>

<style lang="scss" scoped>
.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: var(--rf-size-xl);
  padding: var(--rf-spacing-xl);
}
</style>