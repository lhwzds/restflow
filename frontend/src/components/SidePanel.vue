<script setup lang="ts">
import { DataAnalysis, Expand, Fold, Setting, Share } from '@element-plus/icons-vue'
import { ElAside, ElButton, ElIcon, ElMenu, ElMenuItem } from 'element-plus'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import RestFlowLogo from './RestFlowLogo.vue'

const route = useRoute()
const router = useRouter()

// Control panel collapse state
const isCollapsed = ref(false)

// Current active menu
const activeMenu = ref('workflows')

// Computed width based on collapse state
const panelWidth = computed(() => (isCollapsed.value ? '64px' : '200px'))

// Toggle collapse state
const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value
}

// Handle menu selection
const handleMenuSelect = (index: string) => {
  activeMenu.value = index

  // Navigate to the corresponding route
  switch (index) {
    case 'workflows':
      router.push('/workflows')
      break
    case 'workflow':
      router.push('/workflow')
      break
    case 'agents':
      router.push('/agents')
      break
  }
}

// Watch route changes to update active menu
watch(
  () => route.path,
  (path) => {
    if (path.startsWith('/workflow')) {
      activeMenu.value = path === '/workflows' ? 'workflows' : 'workflow'
    } else if (path.startsWith('/agents')) {
      activeMenu.value = 'agents'
    }
  },
  { immediate: true },
)
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

      <el-menu-item index="workflow">
        <el-icon><Share /></el-icon>
        <template #title>Editor</template>
      </el-menu-item>

      <el-menu-item index="agents">
        <el-icon><Setting /></el-icon>
        <template #title>Agents</template>
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
  padding: 10px;
  border-bottom: 1px solid var(--rf-color-border-base);
  height: 50px;
}

.panel-header.collapsed {
  justify-content: center;
  border-bottom: none;
  height: 32px;
  padding: 16px 0 0 0;
}

.collapse-btn {
  margin-left: auto;
  padding: 4px 8px;
}

.collapsed-btn-container {
  display: flex;
  justify-content: center;
  padding: 4px 0 4px 0;
  border-bottom: 1px solid var(--rf-color-border-base);
}

.expand-btn {
  padding: 4px 8px;
}

.panel-menu {
  flex: 1;
  border: none;
}

.panel-menu:not(.el-menu--collapse) {
  width: 200px;
}

.el-menu-item {
  height: 48px;
  line-height: 48px;
}

.el-menu-item.is-active {
  background-color: var(--rf-color-primary-bg-light);
  color: var(--rf-color-primary);
}
</style>
