<script setup lang="ts">
/**
 * Legacy wrapper for ConvertToTaskDialog.
 */
import ConvertToTaskDialog from './ConvertToTaskDialog.vue'

const props = defineProps<{
  open: boolean
  sessionId: string
  sessionName: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()
</script>

<template>
  <ConvertToTaskDialog
    :open="props.open"
    :session-id="props.sessionId"
    :session-name="props.sessionName"
    @update:open="emit('update:open', $event)"
  />
</template>
