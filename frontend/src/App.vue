<script setup lang="ts">
import { ElContainer, ElMain } from 'element-plus'
import SidePanel from './components/SidePanel.vue'
</script>

<template>
  <el-container style="height: 100vh">
    <SidePanel />
    
    <el-main class="app-main">
      <router-view />
    </el-main>
  </el-container>
</template>

<style lang="scss">
.app-main {
  padding: 0;
  background-color: var(--rf-color-bg-page);
}
</style>
