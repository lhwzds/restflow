<script setup lang="ts">
import { Close, EditPen, VideoPause, VideoPlay } from '@element-plus/icons-vue'
import { ElButton, ElCard, ElInput, ElMessage, ElTooltip } from 'element-plus'
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useWorkflowList } from '@/composables/list/useWorkflowList'
import { useWorkflowTriggers } from '@/composables/triggers/useWorkflowTriggers'
import type { Workflow } from '@/types/generated/Workflow'

interface Props {
  workflow: Workflow
  isSelected: boolean
  isActive?: boolean
  hasTrigger: boolean
}

const props = defineProps<Props>()

const emit = defineEmits<{
  select: []
  updated: []
  deleted: [id: string]
}>()

const router = useRouter()
const { deleteWorkflow, renameWorkflow } = useWorkflowList()
const { activateTrigger, deactivateTrigger } = useWorkflowTriggers()

const isEditing = ref(false)
const editingName = ref('')

function handleCardClick() {
  emit('select')
}

function handleCardDoubleClick() {
  router.push(`/workflow/${props.workflow.id}`)
}

function startRename(event: Event) {
  event.stopPropagation()
  isEditing.value = true
  editingName.value = props.workflow.name
  
  setTimeout(() => {
    const input = document.querySelector(`#rename-input-${props.workflow.id}`) as HTMLInputElement
    if (input) {
      input.focus()
      input.select()
    }
  }, 50)
}

async function saveRename() {
  if (!editingName.value?.trim()) {
    ElMessage.error('Please enter a workflow name')
    return
  }

  const result = await renameWorkflow(props.workflow.id, editingName.value)
  if (result.success) {
    isEditing.value = false
    emit('updated')
  }
}

function cancelRename() {
  isEditing.value = false
  editingName.value = ''
}

function handleRenameKeydown(event: Event | KeyboardEvent) {
  if ('key' in event) {
    if (event.key === 'Enter') {
      event.preventDefault()
      saveRename()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      cancelRename()
    }
  }
}

async function handleDelete(event: Event) {
  event.stopPropagation()
  
  const result = await deleteWorkflow(
    props.workflow.id,
    `Are you sure you want to delete workflow "${props.workflow.name}"?`
  )
  
  if (result.success) {
    emit('deleted', props.workflow.id)
  }
}

async function handleToggleTrigger(event: Event, value: boolean) {
  event.stopPropagation()
  
  try {
    if (value) {
      await activateTrigger(props.workflow.id)
      ElMessage.success('Trigger activated')
    } else {
      await deactivateTrigger(props.workflow.id)
      ElMessage.success('Trigger paused')
    }
    emit('updated')
  } catch (error) {
    ElMessage.error('Operation failed, please try again')
  }
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
        <div v-if="isEditing" class="rename-input-wrapper">
          <ElInput
            :id="`rename-input-${workflow.id}`"
            v-model="editingName"
            size="small"
            @blur="saveRename"
            @keydown="handleRenameKeydown"
            @click.stop
          />
        </div>
        <h3 v-else class="workflow-name">{{ workflow.name }}</h3>
        <div class="workflow-actions">
          <ElTooltip v-if="!isEditing" content="Rename workflow">
            <ElButton
              :icon="EditPen"
              circle
              plain
              size="small"
              @click="startRename"
            />
          </ElTooltip>
          <ElTooltip
            v-if="hasTrigger && !isEditing"
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
          <ElTooltip v-if="!isEditing" content="Delete workflow">
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
        <span>{{ workflow.nodes?.length || 0 }} nodes</span>
        <span v-if="workflow.edges?.length" class="dot">•</span>
        <span v-if="workflow.edges?.length">{{ workflow.edges.length }} connections</span>
      </div>
    </div>
  </ElCard>
</template>

<style lang="scss" scoped>
.workflow-card {
  cursor: pointer;
  transition: all 0.3s ease;
  height: var(--rf-size-md);
  display: flex;
  flex-direction: column;

  &:hover {
    transform: translateY(var(--rf-transform-lift-sm));
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
    padding: var(--rf-spacing-lg);
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
  margin-bottom: var(--rf-spacing-md);
}

.workflow-name {
  font-size: var(--rf-font-size-md);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
  margin: 0;
  line-height: 1.4;
  word-break: break-word;
}

.workflow-actions {
  display: flex;
  gap: var(--rf-spacing-xs);
  margin-left: var(--rf-spacing-sm);
  flex-shrink: 0;
}

.rename-input-wrapper {
  flex: 1;
  margin-right: var(--rf-spacing-sm);

  :deep(.el-input__wrapper) {
    background-color: var(--rf-color-bg-container);
    box-shadow: 0 0 0 1px var(--rf-color-primary) inset;
  }

  :deep(.el-input__inner) {
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-semibold);
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
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-xs);
  margin-top: auto;

  .dot {
    color: var(--rf-color-text-placeholder);
  }
}
</style>