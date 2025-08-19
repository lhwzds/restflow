<script setup lang="ts">
import { ElEmpty } from 'element-plus'
</script>

<template>
  <div class="agent-management">
    <div class="page-header">
      <h1>Agent Management</h1>
    </div>

    <!-- TODO : implement agent management features -->
    <ElEmpty description="Agent management coming soon">
      <span>This feature is under development</span>
    </ElEmpty>
  </div>
</template>

<style scoped>
.agent-management {
  padding: 20px;
  height: 100%;
}

.page-header {
  margin-bottom: 30px;
}

.page-header h1 {
  margin: 0;
  font-size: 28px;
  font-weight: 600;
}
</style>
