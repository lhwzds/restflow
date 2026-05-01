<script setup lang="ts">
import { Sonner } from '@/components/ui/sonner'
import { ConfirmDialog } from '@/components/ui/confirm-dialog'
</script>

<template>
  <!-- Toast notifications -->
  <Sonner />

  <!-- Global confirm dialog -->
  <ConfirmDialog />

  <!-- All pages now use their own layout -->
  <router-view />
</template>
