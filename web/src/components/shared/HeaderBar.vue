<script setup lang="ts">
import { useSlots, computed } from 'vue'
import { ElHeader, ElButton } from 'element-plus'
import { Sun, Moon, Github } from 'lucide-vue-next'
import { useTheme } from '../../composables/useTheme'

defineProps<{
  title: string
}>()

const { isDark, toggleDark } = useTheme()

const slots = useSlots()
const hasLeftActions = computed(() => !!slots['left-actions'])
</script>

<template>
  <el-header class="header-bar" :class="{ 'has-left-content': hasLeftActions }">
    <div v-if="hasLeftActions" class="header-left">
      <slot name="left-actions" />
    </div>

    <h1 class="header-title">{{ title }}</h1>

    <div class="header-actions">
      <slot name="actions" />

      <el-button
        @click="toggleDark()"
        :icon="isDark ? Sun : Moon"
        circle
        text
        size="large"
        :title="isDark ? 'Switch to light mode' : 'Switch to dark mode'"
      />

      <a
        href="https://github.com/lhwzds/restflow"
        target="_blank"
        rel="noopener noreferrer"
        class="github-link"
        title="View on GitHub"
      >
        <el-button
          :icon="Github"
          circle
          text
          size="large"
        />
      </a>
    </div>
  </el-header>
</template>

<style lang="scss" scoped>
.header-bar {
  height: var(--rf-size-sm);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--rf-spacing-lg);
  padding: 0 var(--rf-spacing-xl);
  background: var(--rf-color-bg-container, #fff);
  border-bottom: 1px solid var(--rf-color-border-base);
  transition: background-color 0.3s;

  &.has-left-content {
    display: grid;
    grid-template-columns: auto 1fr auto;
  }
}

.header-left {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
}

.header-title {
  margin: 0;
  font-size: var(--rf-font-size-2xl);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);

  .has-left-content & {
    justify-self: center;
  }
}

.header-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);

  :deep(.search-input) {
    width: var(--rf-size-xl);
  }
}

.github-link {
  display: flex;
  align-items: center;
  text-decoration: none;
  color: inherit;
}
</style>