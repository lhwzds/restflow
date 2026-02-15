<script setup lang="ts">
import { Sonner } from '@/components/ui/sonner'
import { ConfirmDialog } from '@/components/ui/confirm-dialog'
import { useModelsStore } from '@/stores/modelsStore'

// Initialize models store early to trigger preload
// This ensures models are available when components need them
const modelsStore = useModelsStore()
modelsStore.loadModels()
</script>

<template>
  <!-- Toast notifications -->
  <Sonner />

  <!-- Global confirm dialog -->
  <ConfirmDialog />

  <!-- All pages now use their own layout -->
  <router-view />
</template>
