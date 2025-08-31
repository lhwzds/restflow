<script setup lang="ts">
import { Close, EditPen, VideoPause, VideoPlay } from '@element-plus/icons-vue'
import { ElButton, ElCard, ElTooltip } from 'element-plus'

interface Props {
  workflow: {
    id: string
    name: string
    nodeCount: number
    updatedAt: string
  }
  isSelected: boolean
  isActive?: boolean
  hasTrigger: boolean
}

defineProps<Props>()

const emit = defineEmits<{
  select: []
  open: []
  delete: [event: Event]
  rename: [event: Event]
  toggleTrigger: [value: boolean]
}>()

function handleCardClick() {
  emit('select')
}

function handleCardDoubleClick() {
  emit('open')
}

function handleRename(event: Event) {
  event.stopPropagation()
  emit('rename', event)
}

function handleDelete(event: Event) {
  event.stopPropagation()
  emit('delete', event)
}

function handleToggleTrigger(event: Event, value: boolean) {
  event.stopPropagation()
  emit('toggleTrigger', value)
}
</script>

<template>
  <ElCard
    :class="['workflow-card', { selected: isSelected }]"
    @click="handleCardClick"
    @dblclick="handleCardDoubleClick"
  >
    <div class="workflow-content">
      <div class="workflow-header">
        <h3 class="workflow-name">{{ workflow.name }}</h3>
        <div class="workflow-actions">
          <ElTooltip content="Rename workflow">
            <ElButton
              :icon="EditPen"
              circle
              plain
              size="small"
              @click="handleRename"
            />
          </ElTooltip>
          <ElTooltip
            v-if="hasTrigger"
            :content="isActive ? 'Pause trigger' : 'Start trigger'"
          >
            <ElButton
              :icon="isActive ? VideoPause : VideoPlay"
              circle
              plain
              :type="isActive ? 'success' : 'warning'"
              size="small"
              class="trigger-btn"
              @click="(e) => handleToggleTrigger(e, !isActive)"
            />
          </ElTooltip>
          <ElTooltip content="Delete workflow">
            <ElButton
              :icon="Close"
              circle
              plain
              type="danger"
              size="small"
              class="delete-btn"
              @click="handleDelete"
            />
          </ElTooltip>
        </div>
      </div>

      <div class="metadata">
        <span>{{ workflow.nodeCount }} nodes</span>
        <span class="dot">•</span>
        <span>Updated {{ new Date(workflow.updatedAt).toLocaleDateString() }}</span>
      </div>
    </div>
  </ElCard>
</template>

<style lang="scss" scoped>
.workflow-card {
  cursor: pointer;
  transition: all 0.3s ease;
  height: 140px;
  display: flex;
  flex-direction: column;

  &:hover {
    transform: translateY(-4px);
    box-shadow: var(--rf-shadow-lg);
  }

  &.selected {
    border-color: var(--rf-color-primary);
    box-shadow: 0 0 0 1px var(--rf-color-primary);
  }

  :deep(.el-card__body) {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
  }
}

.workflow-content {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  height: 100%;
}

.workflow-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 12px;
}

.workflow-name {
  font-size: 16px;
  font-weight: 600;
  color: var(--rf-color-text-primary);
  margin: 0;
  line-height: 1.4;
  word-break: break-word;
}

.workflow-actions {
  display: flex;
  gap: 4px;
  opacity: 0;
  transition: opacity 0.2s;
  margin-left: 8px;
  flex-shrink: 0;

  .workflow-card:hover & {
    opacity: 1;
  }
}

.trigger-btn.is-success {
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0% {
    box-shadow: 0 0 0 0 var(--rf-color-success-bg);
  }
  70% {
    box-shadow: 0 0 0 6px transparent;
  }
  100% {
    box-shadow: 0 0 0 0 transparent;
  }
}

.metadata {
  font-size: 12px;
  color: var(--rf-color-text-secondary);
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: auto;

  .dot {
    color: var(--rf-color-text-placeholder);
  }
}
</style>