<script setup lang="ts">
import type { Node } from '@vue-flow/core'
import { ref, computed, watch } from 'vue'
import { ElButton, ElTabs, ElTabPane, ElTooltip, ElMessage } from 'element-plus'
import { Settings, Play, Copy, Trash2, X } from 'lucide-vue-next'
import { AgentConfigForm, HttpConfigForm, PythonConfigForm, TriggerConfigForm } from '../nodes'
import { NODE_TYPE, NODE_TYPE_LABELS, SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants'
import { useSingleNodeExecution } from '../../composables/execution/useSingleNodeExecution'
import { useExecutionStore } from '../../stores/executionStore'

interface Props {
  node: Node | null
  visible: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  'update': [node: Node]
  'delete': [nodeId: string]
  'duplicate': [nodeId: string]
  close: []
}>()

const nodeData = ref<any>({})
const activeTab = ref('config')
const isExecuting = ref(false)

const { executeSingleNode, getMockInput } = useSingleNodeExecution()
const executionStore = useExecutionStore()

const popupStyle = computed(() => ({
  position: 'fixed' as const,
  left: '50%',
  top: '50%',
  transform: 'translate(-50%, -50%)',
  zIndex: 2000
}))

const formatPreview = (value: unknown): string => {
  if (value === null || value === undefined) {
    return 'No data available'
  }

  if (typeof value === 'string') {
    return value
  }

  try {
    return JSON.stringify(value, null, 2)
  } catch (error) {
    return String(value)
  }
}

const inputPreview = computed(() => {
  if (!props.node) {
    return 'No data available'
  }

  const storeResult = executionStore.getNodeResult(props.node.id)
  if (storeResult?.input) {
    return formatPreview(storeResult.input)
  }

  const configuredInput = nodeData.value?.input
  const mockInput = getMockInput(props.node.id)

  return formatPreview(configuredInput ?? mockInput)
})

const outputPreview = computed(() => {
  if (!props.node) {
    return 'No data available'
  }

  const storeResult = executionStore.getNodeResult(props.node.id)
  if (storeResult?.output) {
    return formatPreview(storeResult.output)
  }

  return 'Output will be shown after execution'
})

const nodeTypeLabel = computed(() => {
  if (!props.node) return ''
  return NODE_TYPE_LABELS[props.node.type as keyof typeof NODE_TYPE_LABELS] || props.node.type
})

watch(
  () => props.node,
  (newNode) => {
    if (newNode) {
      nodeData.value = { ...newNode.data }
    }
  },
  { immediate: true }
)

const updateNode = () => {
  if (props.node) {
    const updatedNode = {
      ...props.node,
      data: { ...nodeData.value }
    }
    emit('update', updatedNode)
  }
}

const handleFormUpdate = (data: any) => {
  nodeData.value = { ...nodeData.value, ...data }
  updateNode()
}

const testNode = async () => {
  if (!props.node) return

  isExecuting.value = true

  try {
    await executeSingleNode(props.node.id)
    ElMessage.success(SUCCESS_MESSAGES.TEST_PASSED)
  } catch (error: any) {
    ElMessage.error(ERROR_MESSAGES.NODE_EXECUTION_FAILED + ': ' + (error.message || 'Unknown error'))
  } finally {
    isExecuting.value = false
  }
}

const handleDuplicate = () => {
  if (props.node) {
    emit('duplicate', props.node.id)
  }
}

const handleDelete = () => {
  if (props.node) {
    emit('delete', props.node.id)
    emit('update:visible', false)
  }
}

const handleClose = () => {
  emit('update:visible', false)
  emit('close')
  activeTab.value = 'config'
}

</script>

<template>
  <Teleport to="body">
    <Transition name="popup">
      <div
        v-if="visible && node"
        class="node-config-popup"
        :style="popupStyle"
      >
        <div class="popup-header">
          <div class="header-left">
            <Settings :size="18" />
            <span class="node-type">{{ nodeTypeLabel }}</span>
            <span class="node-label">{{ nodeData.label || node.id }}</span>
          </div>
          <div class="header-actions">
            <ElTooltip content="Test Node" placement="bottom">
              <button
                class="action-btn test-btn"
                @click="testNode"
                :disabled="isExecuting"
              >
                <Play :size="16" />
              </button>
            </ElTooltip>
            <ElTooltip content="Duplicate Node" placement="bottom">
              <button class="action-btn" @click="handleDuplicate">
                <Copy :size="16" />
              </button>
            </ElTooltip>
            <ElTooltip content="Delete Node" placement="bottom">
              <button class="action-btn danger" @click="handleDelete">
                <Trash2 :size="16" />
              </button>
            </ElTooltip>
            <button class="action-btn" @click="handleClose">
              <X :size="16" />
            </button>
          </div>
        </div>

        <div class="popup-content">
          <ElTabs v-model="activeTab">
            <ElTabPane label="Configuration" name="config">
              <div class="config-section">
                <div class="form-group">
                  <label>Node Name</label>
                  <input
                    v-model="nodeData.label"
                    @input="updateNode"
                    placeholder="Enter node name"
                  />
                </div>

                <AgentConfigForm
                  v-if="node.type === NODE_TYPE.AGENT"
                  :modelValue="nodeData"
                  @update:modelValue="handleFormUpdate"
                />

                <HttpConfigForm
                  v-if="node.type === NODE_TYPE.HTTP_REQUEST"
                  :modelValue="nodeData"
                  @update:modelValue="handleFormUpdate"
                />

                <PythonConfigForm
                  v-if="node.type === NODE_TYPE.PYTHON"
                  :modelValue="nodeData"
                  @update:modelValue="handleFormUpdate"
                />

                <TriggerConfigForm
                  v-if="
                    node.type === NODE_TYPE.MANUAL_TRIGGER ||
                    node.type === NODE_TYPE.WEBHOOK_TRIGGER ||
                    node.type === NODE_TYPE.SCHEDULE_TRIGGER
                  "
                  :modelValue="nodeData"
                  :nodeType="node.type"
                  @update:modelValue="handleFormUpdate"
                />
              </div>
            </ElTabPane>

            <ElTabPane label="Input/Output" name="io">
              <div class="io-section">
                <div class="io-group">
                  <h4>Input</h4>
                  <div class="variable-list">
                    <div class="variable-item">
                      <pre class="variable-preview">{{ inputPreview }}</pre>
                    </div>
                    <span class="variable-description">Actual input from execution or configured/mock data</span>
                  </div>
                </div>
                <div class="io-group">
                  <h4>Output</h4>
                  <div class="variable-list">
                    <div class="variable-item">
                      <pre class="variable-preview">{{ outputPreview }}</pre>
                    </div>
                    <span class="variable-description">Actual output from execution</span>
                  </div>
                </div>
              </div>
            </ElTabPane>
          </ElTabs>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style lang="scss" scoped>
.node-config-popup {
  width: var(--rf-size-2xl);
  height: min(var(--rf-size-2xl), 80vh);
  background: var(--rf-color-bg-container);
  border: 1px solid var(--rf-color-border-base);
  border-radius: var(--rf-radius-large);
  box-shadow: var(--rf-shadow-lg);
  display: flex;
  flex-direction: column;
  max-height: 80vh;
  cursor: default;

  .popup-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--rf-spacing-md) var(--rf-spacing-lg);
    border-bottom: 1px solid var(--rf-color-border-light);
    background: var(--rf-color-bg-secondary);
    border-radius: var(--rf-radius-large) var(--rf-radius-large) 0 0;

    .header-left {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-sm);

      .node-type {
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
        background: var(--rf-color-primary-bg-lighter);
        border-radius: var(--rf-radius-small);
      }

      .node-label {
        font-weight: var(--rf-font-weight-semibold);
        color: var(--rf-color-text-primary);
      }
    }

    .header-actions {
      display: flex;
      gap: var(--rf-spacing-xs);

      .action-btn {
        width: var(--rf-size-xs);
        height: var(--rf-size-xs);
        padding: 0;
        border: none;
        background: transparent;
        color: var(--rf-color-text-secondary);
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: var(--rf-radius-small);
        transition: all var(--rf-transition-fast);

        &:hover {
          background: var(--rf-color-bg-page);
          color: var(--rf-color-text-primary);
        }

        &.test-btn {
          color: var(--rf-color-success);

          &:hover {
            background: var(--rf-color-success-bg-lighter);
          }
        }

        &.danger {
          &:hover {
            color: var(--rf-color-danger);
            background: var(--rf-color-danger-bg-lighter);
          }
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }
    }
  }

  .popup-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--rf-spacing-lg);
    cursor: default;

    .config-section, .io-section {
      .form-group {
        margin-bottom: var(--rf-spacing-lg);

        label {
          display: block;
          margin-bottom: var(--rf-spacing-sm);
          font-size: var(--rf-font-size-sm);
          font-weight: var(--rf-font-weight-medium);
          color: var(--rf-color-text-regular);
        }

        input {
          width: 100%;
          padding: var(--rf-spacing-sm) var(--rf-spacing-md);
          border: 1px solid var(--rf-color-border-lighter);
          border-radius: var(--rf-radius-base);
          font-size: var(--rf-font-size-base);
          transition: border-color var(--rf-transition-fast);
          background: var(--rf-color-bg-container);
          color: var(--rf-color-text-primary);

          &:focus {
            outline: none;
            border-color: var(--rf-color-border-focus);
            box-shadow: var(--rf-shadow-focus);
          }
        }
      }
    }

    .io-section {
      .io-group {
        margin-bottom: var(--rf-spacing-xl);

        h4 {
          margin-bottom: var(--rf-spacing-md);
          font-size: var(--rf-font-size-md);
          color: var(--rf-color-text-primary);
        }

        .variable-list {
          .variable-item {
            padding: var(--rf-spacing-md);
            background: var(--rf-color-bg-secondary);
            border-radius: var(--rf-radius-small);
            margin-bottom: var(--rf-spacing-sm);

            .variable-preview {
              width: 100%;
              margin: 0;
              font-family: 'Monaco', 'Courier New', monospace;
              font-size: var(--rf-font-size-xs);
              color: var(--rf-color-primary);
              background: var(--rf-color-primary-bg-lighter);
              padding: var(--rf-spacing-sm);
              border-radius: var(--rf-radius-small);
              white-space: pre-wrap;
              word-break: break-word;
            }
          }

          .variable-description {
            display: block;
            font-size: var(--rf-font-size-xs);
            color: var(--rf-color-text-secondary);
            margin-top: var(--rf-spacing-sm);
            padding-left: var(--rf-spacing-xs);
          }
        }
      }
    }
  }

  :deep(.el-tabs) {
    .el-tabs__nav {
      border-bottom: 1px solid var(--rf-color-border-lighter);
    }

    .el-tabs__item {
      color: var(--rf-color-text-secondary);
      font-size: var(--rf-font-size-sm);

      &.is-active {
        color: var(--rf-color-primary);
      }

      &.is-disabled {
        color: var(--rf-color-text-disabled);
      }
    }

    .el-tabs__content {
      padding-top: var(--rf-spacing-lg);
    }
  }
}

.popup-enter-active,
.popup-leave-active {
  transition: all var(--rf-transition-base);
}

.popup-enter-from {
  opacity: 0;
  transform: translate(-50%, -60%) scale(0.92);
}

.popup-leave-to {
  opacity: 0;
  transform: translate(-50%, -60%) scale(0.92);
}

html.dark {
  .node-config-popup {
    background: var(--rf-color-bg-container);

    .popup-header {
      background: var(--rf-color-bg-secondary);
    }

    input {
      background: var(--rf-color-bg-page);
      color: var(--rf-color-text-primary);
    }
  }
}
</style>
