<script setup lang="ts">
import { ElEmpty } from 'element-plus'
import HeaderBar from '../components/HeaderBar.vue'
</script>

<template>
  <div class="agent-management">
    <HeaderBar title="Agent Management">
      <template #actions>
        <!-- Future actions can be added here -->
      </template>
    </HeaderBar>

    <!-- Empty State -->
    <div class="empty-state">
      <ElEmpty description="Agent management coming soon">
        <span>This feature is under development</span>
      </ElEmpty>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.agent-management {
  padding: 20px;
  height: 100%;
  overflow-y: auto;
  background-color: var(--rf-color-bg-page);
}

.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 60vh;
}
</style>
