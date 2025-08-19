<script setup lang="ts">
import { ElContainer, ElMain } from 'element-plus'
import SidePanel from './components/SidePanel.vue'
</script>

<template>
  <el-container style="height: 100vh">
    <SidePanel />
    
    <el-main style="padding: 0">
      <router-view />
    </el-main>
  </el-container>
</template>

<style>
/* Global styles if needed */
</style>
