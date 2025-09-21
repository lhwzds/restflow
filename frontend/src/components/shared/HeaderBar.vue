<script setup lang="ts">
import { ElHeader, ElButton } from 'element-plus'
import { Sun, Moon } from 'lucide-vue-next'
import { useTheme } from '../../composables/useTheme'

defineProps<{
  title: string
}>()

const { isDark, toggleDark } = useTheme()
</script>

<template>
  <el-header class="header-bar">
    <!-- Left: Title -->
    <h1 class="header-title">{{ title }}</h1>
    
    <!-- Right: Actions + Theme Toggle -->
    <div class="header-actions">
      <!-- Page specific actions via slot -->
      <slot name="actions" />
      
      <!-- Theme toggle always visible -->
      <el-button
        @click="toggleDark()"
        :icon="isDark ? Sun : Moon"
        circle
        text
        size="large"
        :title="isDark ? 'Switch to light mode' : 'Switch to dark mode'"
      />
    </div>
  </el-header>
</template>

<style lang="scss" scoped>
.header-bar {
  height: var(--rf-size-sm);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--rf-spacing-xl);
  background: var(--rf-color-bg-container, #fff);
  border-bottom: 1px solid var(--rf-color-border-base);
  transition: background-color 0.3s;
}

.header-title {
  margin: 0;
  font-size: var(--rf-font-size-2xl);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
}

.header-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
  
  :deep(.search-input) {
    width: var(--rf-size-xl);
  }
}
</style>