<script setup lang="ts">
/**
 * TaskCard Component
 *
 * Displays an agent task card with status, schedule, and action buttons.
 * Supports actions like pause/resume and delete.
 */

import { computed } from 'vue'
import {
  Play,
  Pause,
  Trash2,
  Clock,
  Calendar,
  AlertCircle,
  CheckCircle2,
  Loader2,
  Bell,
  BellOff,
} from 'lucide-vue-next'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { AgentTask } from '@/types/generated/AgentTask'
import type { AgentTaskStatus } from '@/types/generated/AgentTaskStatus'
import { formatSchedule, formatTaskStatus } from '@/api/agent-task'

const props = defineProps<{
  task: AgentTask
  isLoading?: boolean
}>()

const emit = defineEmits<{
  click: [task: AgentTask]
  pause: [task: AgentTask]
  resume: [task: AgentTask]
  delete: [task: AgentTask]
}>()

/**
 * Get badge variant based on task status
 */
function getStatusVariant(
  status: AgentTaskStatus
): 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'info' {
  const variantMap: Record<AgentTaskStatus, 'success' | 'info' | 'default' | 'destructive'> = {
    active: 'success',
    paused: 'info',
    running: 'default',
    completed: 'success',
    failed: 'destructive',
  }
  return variantMap[status] || 'default'
}

/**
 * Get status icon component
 */
function getStatusIcon(status: AgentTaskStatus) {
  const iconMap: Record<AgentTaskStatus, typeof CheckCircle2> = {
    active: CheckCircle2,
    paused: Pause,
    running: Loader2,
    completed: CheckCircle2,
    failed: AlertCircle,
  }
  return iconMap[status] || Clock
}

/**
 * Format timestamp for display
 */
function formatTime(timestamp: number | null): string {
  if (!timestamp) return 'Never'

  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const future = diff < 0

  const absDiff = Math.abs(diff)
  const minutes = Math.floor(absDiff / 60000)
  const hours = Math.floor(absDiff / 3600000)
  const days = Math.floor(absDiff / 86400000)

  if (minutes < 1) return future ? 'Soon' : 'Just now'
  if (minutes < 60) return future ? `In ${minutes}m` : `${minutes}m ago`
  if (hours < 24) return future ? `In ${hours}h` : `${hours}h ago`
  if (days < 7) return future ? `In ${days}d` : `${days}d ago`

  return date.toLocaleDateString()
}

const statusText = computed(() => formatTaskStatus(props.task.status))
const statusVariant = computed(() => getStatusVariant(props.task.status))
const StatusIcon = computed(() => getStatusIcon(props.task.status))
const scheduleText = computed(() => formatSchedule(props.task.schedule))
const lastRunText = computed(() => formatTime(props.task.last_run_at))
const nextRunText = computed(() => formatTime(props.task.next_run_at))

const canPause = computed(() => props.task.status === 'active')
const canResume = computed(() => props.task.status === 'paused')
const isRunning = computed(() => props.task.status === 'running')
const hasNotifications = computed(() => props.task.notification?.telegram_enabled)

function handleClick() {
  emit('click', props.task)
}

function handlePause(e: Event) {
  e.stopPropagation()
  emit('pause', props.task)
}

function handleResume(e: Event) {
  e.stopPropagation()
  emit('resume', props.task)
}

function handleDelete(e: Event) {
  e.stopPropagation()
  emit('delete', props.task)
}
</script>

<template>
  <Card class="task-card" :class="{ 'task-card--loading': isLoading }" @click="handleClick">
    <CardContent class="card-body">
      <!-- Header: Name and Status -->
      <div class="card-header">
        <div class="task-name">
          <component :is="StatusIcon" class="status-icon" :class="`status-icon--${task.status}`" :size="16" />
          <span>{{ task.name }}</span>
        </div>
        <Badge :variant="statusVariant">
          {{ statusText }}
        </Badge>
      </div>

      <!-- Description -->
      <div class="task-description" :class="{ 'no-description': !task.description }">
        {{ task.description || 'No description' }}
      </div>

      <!-- Schedule Info -->
      <div class="schedule-section">
        <div class="schedule-row">
          <Calendar :size="14" />
          <span class="schedule-text">{{ scheduleText }}</span>
        </div>
        <div class="schedule-row">
          <Clock :size="14" />
          <span class="schedule-label">Last run:</span>
          <span class="schedule-value">{{ lastRunText }}</span>
        </div>
        <div v-if="task.next_run_at" class="schedule-row">
          <Clock :size="14" />
          <span class="schedule-label">Next run:</span>
          <span class="schedule-value">{{ nextRunText }}</span>
        </div>
      </div>

      <!-- Stats Row -->
      <div class="stats-section">
        <div class="stat-item stat-item--success">
          <CheckCircle2 :size="12" />
          <span>{{ task.success_count }}</span>
        </div>
        <div class="stat-item stat-item--failed">
          <AlertCircle :size="12" />
          <span>{{ task.failure_count }}</span>
        </div>
        <div class="stat-item stat-item--notification">
          <component :is="hasNotifications ? Bell : BellOff" :size="12" />
          <span>{{ hasNotifications ? 'On' : 'Off' }}</span>
        </div>
      </div>

      <!-- Error Message -->
      <div v-if="task.last_error" class="error-section">
        <AlertCircle :size="12" />
        <span class="error-text">{{ task.last_error }}</span>
      </div>

      <!-- Actions -->
      <div class="card-footer">
        <div class="action-buttons">
          <Button
            v-if="canPause"
            variant="ghost"
            size="icon"
            class="action-btn"
            :disabled="isLoading"
            @click="handlePause"
          >
            <Pause :size="14" />
          </Button>
          <Button
            v-if="canResume"
            variant="ghost"
            size="icon"
            class="action-btn"
            :disabled="isLoading"
            @click="handleResume"
          >
            <Play :size="14" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="action-btn action-btn--danger"
            :disabled="isLoading || isRunning"
            @click="handleDelete"
          >
            <Trash2 :size="14" />
          </Button>
        </div>
      </div>
    </CardContent>
  </Card>
</template>

<style lang="scss" scoped>
.task-card {
  cursor: pointer;
  transition: all var(--rf-transition-base) ease;
  border-radius: var(--rf-radius-base);
  overflow: hidden;
  height: 240px;
  width: 100%;

  &:hover {
    transform: translateY(var(--rf-transform-lift-sm));
    box-shadow: var(--rf-shadow-md);
  }

  &--loading {
    opacity: 0.7;
    pointer-events: none;
  }

  .card-body {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 12px;
    gap: var(--rf-spacing-xs);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    min-height: var(--rf-size-xs);

    .task-name {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      font-size: var(--rf-font-size-sm);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
      flex: 1;
      overflow: hidden;

      span {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .status-icon {
        flex-shrink: 0;

        &--active {
          color: var(--rf-color-success);
        }
        &--paused {
          color: var(--rf-color-info);
        }
        &--running {
          color: var(--rf-color-primary);
          animation: spin 1s linear infinite;
        }
        &--completed {
          color: var(--rf-color-success);
        }
        &--failed {
          color: var(--rf-color-danger);
        }
      }
    }
  }

  .task-description {
    color: var(--rf-color-text-regular);
    font-size: var(--rf-font-size-xs);
    line-height: 1.4;
    height: 32px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;

    &.no-description {
      color: var(--rf-color-text-secondary);
      font-style: italic;
    }
  }

  .schedule-section {
    display: flex;
    flex-direction: column;
    gap: var(--rf-spacing-3xs);

    .schedule-row {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      font-size: var(--rf-font-size-xs);
      color: var(--rf-color-text-secondary);

      .schedule-text {
        color: var(--rf-color-text-regular);
      }

      .schedule-label {
        color: var(--rf-color-text-secondary);
      }

      .schedule-value {
        color: var(--rf-color-text-regular);
        font-weight: var(--rf-font-weight-medium);
      }
    }
  }

  .stats-section {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);
    font-size: var(--rf-font-size-xs);

    .stat-item {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-3xs);

      &--success {
        color: var(--rf-color-success);
      }

      &--failed {
        color: var(--rf-color-danger);
      }

      &--notification {
        color: var(--rf-color-text-secondary);
      }
    }
  }

  .error-section {
    display: flex;
    align-items: flex-start;
    gap: var(--rf-spacing-xs);
    padding: var(--rf-spacing-xs);
    background: var(--rf-color-danger-light);
    border-radius: var(--rf-radius-xs);
    color: var(--rf-color-danger);
    font-size: var(--rf-font-size-xs);

    .error-text {
      flex: 1;
      line-height: 1.3;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }
  }

  .card-footer {
    border-top: 1px solid var(--rf-color-border-lighter);
    padding-top: var(--rf-spacing-xs);
    margin-top: auto;

    .action-buttons {
      display: flex;
      justify-content: flex-end;
      gap: var(--rf-spacing-xs);

      .action-btn {
        height: 28px;
        width: 28px;

        &--danger:hover {
          color: var(--rf-color-danger);
        }
      }
    }
  }
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
