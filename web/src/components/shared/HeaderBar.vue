<script setup lang="ts">
import { ElHeader, ElButton } from 'element-plus'
import { Sun, Moon, Github } from 'lucide-vue-next'
import { useTheme } from '../../composables/useTheme'

defineProps<{
  title: string
}>()

const { isDark, toggleDark } = useTheme()
</script>

<template>
  <el-header class="header-bar">
    <div class="header-left">
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
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: var(--rf-spacing-lg);
  padding: 0 var(--rf-spacing-xl);
  background: var(--rf-color-bg-container, #fff);
  border-bottom: 1px solid var(--rf-color-border-base);
  transition: background-color 0.3s;
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
  justify-self: center;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
  justify-self: end;

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