<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  variant?: 'default' | 'fullheight' | 'split'
  noPadding?: boolean
  noHeader?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  noPadding: false,
  noHeader: false
})

const layoutClasses = computed(() => ({
  'page-layout': true,
  [`page-layout--${props.variant}`]: true,
  'page-layout--no-padding': props.noPadding
}))
</script>

<template>
  <div :class="layoutClasses">
    <header v-if="!noHeader && $slots.header" class="page-layout__header">
      <slot name="header" />
    </header>

    <main class="page-layout__content">
      <slot />
    </main>

    <footer v-if="$slots.footer" class="page-layout__footer">
      <slot name="footer" />
    </footer>
  </div>
</template>

<style lang="scss" scoped>
.page-layout {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--rf-color-bg-page);

  &--default {
    .page-layout__content {
      flex: 1;
      padding: var(--rf-page-padding, 20px);
      overflow-y: auto;
    }
  }

  &--fullheight {
    .page-layout__content {
      flex: 1;
      min-height: 0;
      display: flex;
      flex-direction: column;
    }
  }

  &--split {
    overflow: hidden;

    .page-layout__content {
      flex: 1;
      min-height: 0;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
  }

  &--no-padding {
    .page-layout__content {
      padding: 0;
    }
  }

  &__header {
    flex-shrink: 0;
    background: var(--rf-color-bg-container);
    border-bottom: 1px solid var(--rf-color-border-lighter);
  }

  &__footer {
    flex-shrink: 0;
    background: var(--rf-color-bg-container);
    border-top: 1px solid var(--rf-color-border-lighter);
    padding: var(--rf-spacing-lg) var(--rf-spacing-xl);
  }
}

html.dark {
  .page-layout {
    &__header,
    &__footer {
      background: var(--rf-color-bg-container);
    }
  }
}
</style>