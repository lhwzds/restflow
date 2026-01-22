<script setup lang="ts">
import { Settings, Lock, FileText, ChevronLeft, ChevronRight } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import RestFlowLogo from './RestFlowLogo.vue'
import { cn } from '@/lib/utils'

const route = useRoute()
const router = useRouter()

const isCollapsed = ref(false)

const activeMenu = computed(() => {
  const path = route.path
  if (path.startsWith('/agents')) return 'agents'
  if (path.startsWith('/secrets')) return 'secrets'
  if (path.startsWith('/skills')) return 'skills'
  return 'agents'
})

const toggleCollapse = () => {
  isCollapsed.value = !isCollapsed.value
}

const handleMenuSelect = (index: string) => {
  switch (index) {
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

const menuItems = [
  { key: 'agents', label: 'Agents', icon: Settings },
  { key: 'secrets', label: 'Secrets', icon: Lock },
  { key: 'skills', label: 'Skills', icon: FileText },
]
</script>

<template>
  <aside :class="cn('side-panel', isCollapsed && 'collapsed')">
    <div class="panel-header" :class="{ collapsed: isCollapsed }" @click="toggleCollapse">
      <div class="logo-wrapper">
        <RestFlowLogo :show-text="true" :icon-size="36" :text-size="24" :gap="10" />
      </div>
      <div class="collapse-indicator">
        <ChevronRight v-if="isCollapsed" :size="16" />
        <ChevronLeft v-else :size="16" />
      </div>
    </div>

    <!-- Menu -->
    <nav class="panel-menu">
      <button
        v-for="item in menuItems"
        :key="item.key"
        :class="cn('menu-item', activeMenu === item.key && 'active')"
        @click="handleMenuSelect(item.key)"
      >
        <component :is="item.icon" :size="20" class="menu-icon" />
        <span v-if="!isCollapsed" class="menu-label">{{ item.label }}</span>
      </button>
    </nav>
  </aside>
</template>

<style lang="scss" scoped>
.side-panel {
  width: var(--rf-size-lg);
  background-color: var(--rf-color-bg-container);
  border-right: 1px solid var(--rf-color-border-base);
  transition: width 0.3s ease;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;

  &.collapsed {
    width: var(--rf-size-sm);
  }
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-lg) var(--rf-spacing-xl);
  border-bottom: 1px solid var(--rf-color-border-base);
  height: 60px;
  cursor: pointer;
  overflow: hidden;

  &:hover {
    background-color: var(--rf-color-bg-secondary);

    .collapse-indicator {
      opacity: 1;
    }
  }

  &.collapsed {
    justify-content: center;
    padding: var(--rf-spacing-lg) var(--rf-spacing-sm);

    .logo-wrapper {
      :deep(.logo-text) {
        width: 0;
        opacity: 0;
        margin-left: 0 !important;
      }
    }

    .collapse-indicator {
      position: absolute;
      right: var(--rf-spacing-sm);
    }
  }
}

.logo-wrapper {
  display: flex;
  align-items: center;
  overflow: hidden;

  :deep(.logo-text) {
    transition: width 0.3s ease, opacity 0.3s ease, margin-left 0.3s ease;
    white-space: nowrap;
    overflow: hidden;
  }
}

.collapse-indicator {
  opacity: 0;
  transition: opacity 0.2s ease;
  color: var(--rf-color-text-secondary);
  flex-shrink: 0;
}

.panel-menu {
  flex: 1;
  padding: var(--rf-spacing-sm) 0;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
  width: 100%;
  padding: var(--rf-spacing-md) var(--rf-spacing-lg);
  border: none;
  background: transparent;
  cursor: pointer;
  color: var(--rf-color-text-regular);
  font-size: var(--rf-font-size-sm);
  transition: all 0.2s ease;
  text-align: left;

  .collapsed & {
    justify-content: center;
    padding: var(--rf-spacing-md);
  }

  &:hover {
    background-color: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-primary);
  }

  &.active {
    background-color: var(--rf-color-primary-bg-light);
    color: var(--rf-color-primary);

    .menu-icon {
      color: var(--rf-color-primary);
    }
  }
}

.menu-icon {
  flex-shrink: 0;
}

.menu-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
