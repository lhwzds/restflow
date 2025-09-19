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
  height: var(--rf-page-header-height, 60px);
  padding: 0 var(--rf-spacing-xl, 20px);
  background: var(--rf-color-bg-container);

  &__left {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md, 12px);
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
    font-size: var(--rf-font-size-lg, 18px);
    font-weight: var(--rf-font-weight-semibold, 600);
    color: var(--rf-color-text-primary);
    line-height: 1.2;
  }

  &__subtitle {
    margin-top: 2px;
    font-size: var(--rf-font-size-sm, 13px);
    color: var(--rf-color-text-secondary);
  }

  &__actions {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md, 12px);

    :deep(.el-input) {
      width: 300px;
    }
  }
}

@media (max-width: 768px) {
  .page-header {
    padding: 0 var(--rf-spacing-lg, 16px);

    &__title {
      font-size: var(--rf-font-size-md, 16px);
    }

    &__actions {
      :deep(.el-input) {
        width: 200px;
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