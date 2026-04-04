<script setup lang="ts">
import TaskStatusBadge from '@/components/task/TaskStatusBadge.vue'
import type { TaskStatus } from '@/types/generated/TaskStatus'

defineProps<{
  status: TaskStatus
}>()
</script>

<template>
  <TaskStatusBadge v-bind="$props" />
</template>
