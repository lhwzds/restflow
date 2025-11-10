<template>
  <div class="execution-history-panel">
    <div class="panel-header">
      <div class="panel-title">
        <span class="panel-title-text">Execution History</span>
        <span class="panel-title-meta">({{ totalExecutions }} total)</span>
      </div>
      <el-button
        type="primary"
        :icon="Refresh"
        circle
        size="small"
        :loading="isLoading"
        @click="loadHistory(page)"
      />
    </div>

    <div v-if="executions.length === 0 && !isLoading" class="empty-state">
      <p>No execution records</p>
    </div>

    <div v-else class="execution-list">
      <div
        v-for="execution in executions"
        :key="execution.execution_id"
        class="execution-item"
        :class="{
          'is-active': selectedExecutionId === execution.execution_id,
          [`status-${execution.status.toLowerCase()}`]: true,
        }"
        @click="handleExecutionClick(execution.execution_id)"
      >
        <div class="execution-header">
          <div class="header-left">
            <span class="status-icon">{{ getStatusIcon(execution.status) }}</span>
            <span class="execution-id">{{ truncateId(execution.execution_id) }}</span>
          </div>
          <div class="header-right">
            <span class="status-text">{{ getStatusText(execution.status) }}</span>
          </div>
        </div>

        <div class="execution-details">
          <div class="details-left">
            <el-tooltip :content="formatFullDateTime(Number(execution.started_at))" placement="top">
              <span class="time-text">{{ formatRelativeTime(Number(execution.started_at)) }}</span>
            </el-tooltip>
          </div>
          <div class="details-right">
            <span v-if="isTestExecution(execution.execution_id)" class="test-badge"> Test </span>
            <span class="task-count">
              {{ execution.completed_tasks }}/{{ execution.total_tasks }} tasks
              <span v-if="execution.failed_tasks > 0" class="failed-count">
                ({{ execution.failed_tasks }} failed)
              </span>
            </span>
          </div>
        </div>

        <div v-if="execution.status === 'Running'" class="progress-bar">
          <div
            class="progress-fill"
            :style="{
              width: `${execution.total_tasks > 0 ? (execution.completed_tasks / execution.total_tasks) * 100 : 0}%`,
            }"
          />
        </div>
      </div>
    </div>

    <div class="panel-footer" v-if="totalExecutions > 0">
      <el-pagination
        class="panel-pagination"
        layout="prev, pager, next, jumper"
        :current-page="page"
        :total="totalExecutions"
        :page-size="pageSize"
        :pager-count="5"
        :small="true"
        :disabled="isLoading"
        @current-change="handlePageChange"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { ElTooltip } from 'element-plus'
import { useExecutionHistory } from '../composables/execution/useExecutionHistory'

const props = defineProps<{
  workflowId: string
}>()

const workflowIdRef = computed(() => props.workflowId)

const {
  executions,
  isLoading,
  selectedExecutionId,
  totalExecutions,
  page,
  pageSize,
  loadHistory,
  switchToExecution,
  getStatusText,
  getStatusIcon,
  formatRelativeTime,
  formatFullDateTime,
  startPolling,
  stopPolling,
  goToPage,
} = useExecutionHistory(workflowIdRef)

const truncateId = (id: string): string => {
  return id.length > 24 ? `${id.substring(0, 24)}…` : id
}

const isTestExecution = (executionId: string): boolean => {
  return executionId.startsWith('test-')
}

const handleExecutionClick = (executionId: string) => {
  switchToExecution(executionId)
}

onMounted(() => {
  startPolling()
})

onUnmounted(() => {
  stopPolling()
})

const handlePageChange = (newPage: number) => {
  if (newPage === page.value) return
  goToPage(newPage)
}
</script>

<style scoped lang="scss">
.execution-history-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  max-height: 100%;
  background: color-mix(in srgb, var(--rf-color-bg-container) 88%, transparent);
  border: 1px solid var(--rf-color-border-light);
  border-radius: var(--rf-radius-large);
  overflow: hidden;
  box-shadow: var(--rf-shadow-xl);
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--rf-spacing-md);
  border-bottom: 1px solid var(--rf-color-border-light);
  background: var(--rf-color-bg-secondary);
}

.panel-title {
  display: flex;
  align-items: baseline;
  gap: var(--rf-spacing-xs);
}

.panel-title-text {
  margin: 0;
  font-size: var(--rf-font-size-lg);
  font-weight: 600;
  color: var(--rf-color-text-primary);
}

.panel-title-meta {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  color: var(--rf-color-text-secondary);
  font-size: var(--rf-font-size-base);
}

.execution-list {
  flex: 1;
  overflow-y: auto;
  padding: var(--rf-spacing-sm);
}

.execution-item {
  backdrop-filter: blur(8px);
  padding: var(--rf-spacing-md);
  margin-bottom: var(--rf-spacing-sm);
  border: 1px solid var(--rf-color-border-light);
  border-radius: var(--rf-radius-small);
  cursor: pointer;
  transition: all var(--rf-transition-base);
  background: var(--rf-color-bg-page);

  &:hover {
    border-color: var(--rf-color-primary);
    box-shadow: var(--rf-shadow-sm);
  }

  &.is-active {
    border-color: var(--rf-color-primary);
    background: var(--rf-color-primary-light);
  }

  &.status-running {
    border-left: 3px solid var(--rf-color-warning);
  }

  &.status-completed {
    border-left: 3px solid var(--rf-color-success);
  }

  &.status-failed {
    border-left: 3px solid var(--rf-color-danger);
  }
}

.execution-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--rf-spacing-xs);
  margin-bottom: var(--rf-spacing-xs);
}

.header-left {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-xs);
}

.header-right {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-xs);
}

.status-icon {
  font-size: var(--rf-font-size-lg);
  line-height: 1;
}

.execution-id {
  font-family: monospace;
  font-size: var(--rf-font-size-sm);
  color: var(--rf-color-text-regular);
}

.status-text {
  margin-left: auto;
  font-size: var(--rf-font-size-sm);
  font-weight: 500;
  color: var(--rf-color-text-secondary);
}

.execution-details {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--rf-font-size-sm);
  color: var(--rf-color-text-secondary);
}

.time-text {
  opacity: 0.8;
}

.task-count {
  font-weight: 500;
}

.failed-count {
  color: var(--rf-color-danger);
}

.progress-bar {
  margin-top: var(--rf-spacing-xs);
  height: 4px;
  background: var(--rf-color-border-lighter);
  border-radius: 2px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--rf-color-primary);
  transition: width 0.3s ease;
}

.details-right {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-sm);
}

.test-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: var(--rf-spacing-3xs) var(--rf-spacing-sm);
  border-radius: var(--rf-radius-pill);
  background: var(--rf-color-warning-light);
  color: var(--rf-color-warning);
  font-size: var(--rf-font-size-2xs);
  font-weight: var(--rf-font-weight-medium);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.panel-footer {
  border-top: 1px solid var(--rf-color-border-light);
  background: color-mix(in srgb, var(--rf-color-bg-container) 92%, transparent);
  padding: var(--rf-spacing-xs) var(--rf-spacing-md);
}

.panel-pagination {
  width: 100%;
  display: flex;
  justify-content: center;
}
</style>
