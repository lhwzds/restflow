<script setup lang="ts">
import { useToast } from './use-toast'
import Toast from './Toast.vue'
import ToastClose from './ToastClose.vue'
import ToastDescription from './ToastDescription.vue'
import ToastProvider from './ToastProvider.vue'
import ToastTitle from './ToastTitle.vue'
import ToastViewport from './ToastViewport.vue'

const { toasts } = useToast()
</script>

<template>
  <ToastProvider>
    <Toast v-for="toast in toasts" :key="toast.id" :variant="toast.variant">
      <div class="grid gap-1">
        <ToastTitle v-if="toast.title">
          {{ toast.title }}
        </ToastTitle>
        <ToastDescription v-if="toast.description">
          {{ toast.description }}
        </ToastDescription>
      </div>
      <component :is="toast.action" v-if="toast.action" />
      <ToastClose />
    </Toast>
    <ToastViewport />
  </ToastProvider>
</template>
