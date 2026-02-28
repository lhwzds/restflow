<script setup lang="ts">
import { ref } from 'vue'
import { useConfirm } from '@/composables/useConfirm'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from '@/components/ui/alert-dialog'

const { isOpen, options, handleConfirm, handleCancel } = useConfirm()

const confirmed = ref(false)

function onConfirmClick() {
  confirmed.value = true
  handleConfirm()
}

function onOpenChange(open: boolean) {
  if (!open && !confirmed.value) {
    handleCancel()
  }
  confirmed.value = false
}
</script>

<template>
  <AlertDialog :open="isOpen" @update:open="onOpenChange">
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{{ options.title }}</AlertDialogTitle>
        <AlertDialogDescription>
          {{ options.description }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel @click="handleCancel">
          {{ options.cancelText }}
        </AlertDialogCancel>
        <AlertDialogAction
          :class="
            options.variant === 'destructive'
              ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
              : ''
          "
          @click="onConfirmClick"
        >
          {{ options.confirmText }}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
