<script setup lang="ts">
import { ElButton } from 'element-plus'
import { ArrowLeft } from '@element-plus/icons-vue'
import { useRouter } from 'vue-router'

interface Props {
  title: string
  subtitle?: string
  showBack?: boolean
  backTo?: string | (() => void)
}

const props = defineProps<Props>()
const router = useRouter()

function handleBack() {
  if (typeof props.backTo === 'function') {
    props.backTo()
  } else if (typeof props.backTo === 'string') {
    router.push(props.backTo)
  } else {
    router.back()
  }
}
</script>

<template>
  <div class="page-header">
    <div class="page-header__left">
      <ElButton
        v-if="showBack"
        :icon="ArrowLeft"
        @click="handleBack"
        class="page-header__back"
      >
        Back
      </ElButton>

      <div class="page-header__title-group">
        <h1 class="page-header__title">{{ title }}</h1>
        <span v-if="subtitle" class="page-header__subtitle">{{ subtitle }}</span>
      </div>
    </div>

    <div v-if="$slots.actions" class="page-header__actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<style lang="scss" scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--rf-page-header-height, var(--rf-size-sm));
  padding: 0 var(--rf-spacing-xl);
  background: var(--rf-color-bg-container);

  &__left {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);
  }

  &__back {
    flex-shrink: 0;
  }

  &__title-group {
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  &__title {
    margin: 0;
    font-size: var(--rf-font-size-lg);
    font-weight: var(--rf-font-weight-semibold);
    color: var(--rf-color-text-primary);
    line-height: 1.2;
  }

  &__subtitle {
    margin-top: var(--rf-spacing-3xs);
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-text-secondary);
  }

  &__actions {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);

    :deep(.el-input) {
      width: var(--rf-size-xl);
    }
  }
}

@media (max-width: 768px) {
  .page-header {
    padding: 0 var(--rf-spacing-lg);

    &__title {
      font-size: var(--rf-font-size-md);
    }

    &__actions {
      :deep(.el-input) {
        width: var(--rf-size-lg);
      }
    }
  }
}

html.dark {
  .page-header {
    background: var(--rf-color-bg-container);
  }
}
</style>