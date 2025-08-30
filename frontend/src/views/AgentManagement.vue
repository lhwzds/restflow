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

    <!-- TODO : implement agent management features -->
    <ElEmpty description="Agent management coming soon">
      <span>This feature is under development</span>
    </ElEmpty>
  </div>
</template>

<style lang="scss" scoped>
.agent-management {
  padding: 20px;
  height: 100%;
  background-color: var(--rf-color-bg-page);
}

</style>
