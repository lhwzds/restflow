<script setup lang="ts">
import { computed } from 'vue'
import { Button } from '@/components/ui/button'
import { Inbox } from 'lucide-vue-next'

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
  itemName: 'item',
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

const computedActionVariant = computed(() => {
  return props.searchQuery ? 'outline' : 'default'
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
    <div class="empty-state__content">
      <Inbox class="empty-state__icon" :size="48" />
      <p class="empty-state__description">{{ computedDescription }}</p>
      <Button :variant="computedActionVariant" @click="handleAction">
        {{ computedActionText }}
      </Button>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: var(--rf-size-xl);
  padding: var(--rf-spacing-xl);

  &__content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--rf-spacing-lg);
  }

  &__icon {
    color: var(--rf-color-text-secondary);
    opacity: 0.5;
  }

  &__description {
    color: var(--rf-color-text-secondary);
    font-size: var(--rf-font-size-sm);
    text-align: center;
    margin: 0;
  }
}
</style>
