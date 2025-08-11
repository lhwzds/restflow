<script setup lang="ts">
import WorkflowEditor from './components/WorkflowEditor.vue'
</script>

<template>
  <el-container style="height: 100vh">
    <el-aside width="250px" style="background-color: #f0f2f5; padding: 20px">
      <h3>RestFlow</h3>
      <el-menu>
        <el-menu-item index="1">
          <span>Workflow 1</span>
        </el-menu-item>
        <el-menu-item index="2">
          <span>Workflow 2</span>
        </el-menu-item>
      </el-menu>
    </el-aside>

    <el-main style="padding: 0">
      <WorkflowEditor />
    </el-main>
  </el-container>
</template>

<style>
.el-aside {
  border-right: 1px solid #e6e6e6;
}
</style>
