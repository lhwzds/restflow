<script setup lang="ts">
import SidePanel from './components/shared/SidePanel.vue'
import { Sonner } from '@/components/ui/sonner'
import { ConfirmDialog } from '@/components/ui/confirm-dialog'
</script>

<template>
  <!-- Toast notifications -->
  <Sonner />

  <!-- Global confirm dialog -->
  <ConfirmDialog />

  <div class="app-container">
    <SidePanel />

    <main class="app-main">
      <router-view />
    </main>
  </div>
</template>

<style lang="scss">
.app-container {
  display: flex;
  height: 100vh;
}

.app-main {
  flex: 1;
  overflow: auto;
  padding: 0;
  background-color: var(--rf-color-bg-page);
}
</style>
