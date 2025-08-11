<script setup lang="ts">
import { ElContainer, ElMain } from 'element-plus'
import WorkflowEditor from './components/WorkflowEditor.vue'
import SidePanel from './components/SidePanel.vue'
</script>

<template>
  <el-container style="height: 100vh">
    <SidePanel />
    
    <el-main style="padding: 0">
      <WorkflowEditor />
    </el-main>
  </el-container>
</template>

<style>
/* Global styles if needed */
</style>
