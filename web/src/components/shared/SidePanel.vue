<script setup lang="ts">
import { DataAnalysis, Expand, Fold, Setting, Lock, Document } from '@element-plus/icons-vue'
import { ElAside, ElButton, ElIcon, ElMenu, ElMenuItem } from 'element-plus'
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import RestFlowLogo from './RestFlowLogo.vue'

const route = useRoute()
const router = useRouter()

const isCollapsed = ref(false)

const activeMenu = computed(() => {
  const path = route.path
  if (path === '/workflows') return 'workflows'
  if (path.startsWith('/workflow')) return 'workflows' // Editor is part of workflows
  if (path.startsWith('/agents')) return 'agents'
  if (path.startsWith('/secrets')) return 'secrets'
  if (path.startsWith('/skills')) return 'skills'
  return 'workflows'
})

const panelWidth = computed(() => (isCollapsed.value ? 'var(--rf-size-sm)' : 'var(--rf-size-lg)'))

const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value
}

const handleMenuSelect = (index: string) => {
  switch (index) {
    case 'workflows':
      router.push('/workflows')
      break
    case 'agents':
      router.push('/agents')
      break
    case 'secrets':
      router.push('/secrets')
      break
    case 'skills':
      router.push('/skills')
      break
  }
}
</script>

<template>
  <el-aside :width="panelWidth" class="side-panel">
    <div class="panel-header" :class="{ collapsed: isCollapsed }">
      <RestFlowLogo :show-text="!isCollapsed" :icon-size="32" :text-size="20" :gap="8" />
      <el-button
        v-if="!isCollapsed"
        :icon="Fold"
        size="large"
        text
        @click="toggleCollapse"
        class="collapse-btn"
      />
    </div>

    <div v-if="isCollapsed" class="collapsed-btn-container">
      <el-button :icon="Expand" size="large" text @click="toggleCollapse" class="expand-btn" />
    </div>

    <!-- Menu -->
    <el-menu
      :default-active="activeMenu"
      class="panel-menu"
      :collapse="isCollapsed"
      @select="handleMenuSelect"
    >
      <el-menu-item index="workflows">
        <el-icon><DataAnalysis /></el-icon>
        <template #title>Workflows</template>
      </el-menu-item>

      <el-menu-item index="agents">
        <el-icon><Setting /></el-icon>
        <template #title>Agents</template>
      </el-menu-item>

      <el-menu-item index="secrets">
        <el-icon><Lock /></el-icon>
        <template #title>Secrets</template>
      </el-menu-item>

      <el-menu-item index="skills">
        <el-icon><Document /></el-icon>
        <template #title>Skills</template>
      </el-menu-item>
    </el-menu>
  </el-aside>
</template>

<style lang="scss" scoped>
.side-panel {
  background-color: var(--rf-color-bg-container, #fff);
  border-right: 1px solid var(--rf-color-border-base);
  transition: width 0.3s ease;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-md);
  border-bottom: 1px solid var(--rf-color-border-base);
  height: var(--rf-size-xs);
}

.panel-header.collapsed {
  justify-content: center;
  border-bottom: none;
  height: 32px;
  padding: var(--rf-spacing-lg) 0 0 0;
}

.collapse-btn {
  margin-left: auto;
  padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
}

.collapsed-btn-container {
  display: flex;
  justify-content: center;
  padding: var(--rf-spacing-xs) 0 var(--rf-spacing-xs) 0;
  border-bottom: 1px solid var(--rf-color-border-base);
}

.expand-btn {
  padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
}

.panel-menu {
  flex: 1;
  border: none;
}

.panel-menu:not(.el-menu--collapse) {
  width: var(--rf-size-lg);
}

.el-menu-item {
  height: var(--rf-size-line-lg);
  line-height: var(--rf-size-line-lg);
}

.el-menu-item.is-active {
  background-color: var(--rf-color-primary-bg-light);
  color: var(--rf-color-primary);
}
</style>
