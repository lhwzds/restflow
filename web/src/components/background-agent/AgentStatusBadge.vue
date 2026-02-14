<script setup lang="ts">
import { computed } from 'vue'
import { Badge, type BadgeVariants } from '@/components/ui/badge'
import type { BackgroundAgentStatus } from '@/types/generated/BackgroundAgentStatus'

const props = defineProps<{
  status: BackgroundAgentStatus
}>()

const badgeConfig = computed<{
  variant: NonNullable<BadgeVariants['variant']>
  label: string
  pulse: boolean
}>(() => {
  switch (props.status) {
    case 'running':
      return { variant: 'default', label: 'Running', pulse: true }
    case 'active':
      return { variant: 'info', label: 'Active', pulse: false }
    case 'paused':
      return { variant: 'warning', label: 'Paused', pulse: false }
    case 'completed':
      return { variant: 'success', label: 'Completed', pulse: false }
    case 'failed':
      return { variant: 'destructive', label: 'Failed', pulse: false }
    case 'interrupted':
      return { variant: 'outline', label: 'Interrupted', pulse: false }
    default:
      return { variant: 'secondary', label: props.status, pulse: false }
  }
})
</script>

<template>
  <Badge :variant="badgeConfig.variant" :class="badgeConfig.pulse ? 'animate-pulse' : ''">
    {{ badgeConfig.label }}
  </Badge>
</template>
