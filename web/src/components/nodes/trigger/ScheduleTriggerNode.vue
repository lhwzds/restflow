<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Clock, Calendar } from 'lucide-vue-next'
import BaseNode from '@/components/nodes/BaseNode.vue'
import BaseTriggerNode from './BaseTriggerNode.vue'

interface ScheduleTriggerData {
  label?: string
  cron?: string
  timezone?: string
}

const props = defineProps<NodeProps<ScheduleTriggerData>>()

const emit = defineEmits<{
  'open-config': []
  'test-node': []
  'updateNodeInternals': [nodeId: string]
}>()

// Format the cron expression into a human-friendly string
const formatCron = (cron?: string): string => {
  if (!cron) return 'Not configured'

  // Basic cron expression recognition
  const patterns: Record<string, string> = {
    '* * * * *': 'Every minute',
    '0 * * * *': 'Every hour',
    '0 0 * * *': 'Daily at midnight',
    '0 9 * * *': 'Daily at 9:00 AM',
    '0 0 * * 0': 'Weekly (Sunday)',
    '0 0 1 * *': 'Monthly (1st day)',
  }

  return patterns[cron] || cron
}
</script>

<template>
  <BaseTriggerNode>
    <BaseNode
      :node-props="props"
      node-class="schedule-trigger-node"
      default-label="Schedule"
      action-button-tooltip="Test Schedule"
      @open-config="emit('open-config')"
      @action-button="emit('test-node')"
      @updateNodeInternals="emit('updateNodeInternals', $event)"
    >
      <template #icon>
        <Clock :size="24" />
        <Calendar :size="12" class="icon-decoration" />
      </template>

      <template #badge>
        <div class="schedule-info">
          <div v-if="props.data?.cron" class="cron-badge">
            {{ formatCron(props.data.cron) }}
          </div>
          <div v-if="props.data?.timezone" class="timezone-badge">
            {{ props.data.timezone }}
          </div>
        </div>
      </template>
    </BaseNode>
  </BaseTriggerNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-primary);

.schedule-trigger-node {
  @include node-handle($node-color);

  .node-body {
    @include node-glass($node-color);

    &:hover {
      box-shadow:
        0 6px 20px rgba($node-color, 0.3),
        inset 0 0 0 1px rgba($node-color, 0.2);
    }
  }

  .node-icon {
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-primary));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-primary);

.icon-decoration {
  position: absolute;
  top: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-success);
}

.schedule-info {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-2xs);
}

.cron-badge {
  font-size: var(--rf-font-size-xs);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
  font-weight: var(--rf-font-weight-medium);
}

.timezone-badge {
  font-size: var(--rf-font-size-2xs);
  color: var(--rf-color-text-secondary);
  background: var(--rf-color-bg-secondary);
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
  text-align: center;
}
</style>
