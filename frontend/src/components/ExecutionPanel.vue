<script setup lang="ts">
import { computed, ref, onUnmounted } from 'vue'
import { 
  ElCard, 
  ElButton, 
  ElTag, 
  ElDescriptions, 
  ElDescriptionsItem,
  ElCollapse,
  ElCollapseItem,
  ElEmpty,
  ElAlert
} from 'element-plus'
import { ArrowDown, ArrowUp, Close, CopyDocument } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { useExecutionStore } from '../stores/executionStore'
import { useKeyboardShortcuts } from '../composables/shared/useKeyboardShortcuts'
import { useExecutionPanelResize } from '../composables/ui/useExecutionPanelResize'

const executionStore = useExecutionStore()

const panelRef = ref<HTMLElement>()

// Hook for handling panel resize with drag-and-drop functionality
const { isResizing, startResize, stopResize } = useExecutionPanelResize(panelRef)

const isOpen = computed(() => executionStore.panelState.isOpen)
const panelHeight = computed(() => {
  if (!isOpen.value) return '48px'
  return `${executionStore.panelState.height}%`
})

const executionSummary = computed(() => executionStore.executionSummary)
const selectedResult = computed(() => executionStore.selectedNodeResult)
const hasResults = computed(() => executionStore.hasResults)

const togglePanel = () => {
  executionStore.togglePanel()
}

const closePanel = () => {
  executionStore.closePanel()
}

const formatJson = (data: any): string => {
  if (data === undefined || data === null) return 'null'
  if (typeof data === 'string') return data
  try {
    return JSON.stringify(data, null, 2)
  } catch (error) {
    return String(data)
  }
}

const copyToClipboard = async (text: string) => {
  try {
    await navigator.clipboard.writeText(text)
    ElMessage.success('Copied to clipboard')
  } catch (error) {
    ElMessage.error('Failed to copy')
  }
}

// Keyboard shortcuts for panel control (Ctrl/Cmd+J toggle, Esc close)
useKeyboardShortcuts({
  'ctrl+j': togglePanel,
  'meta+j': togglePanel,
  'escape': () => {
    if (isOpen.value) closePanel()
  }
})

const formatTimestamp = (timestamp?: number) => {
  if (!timestamp) return 'N/A'
  const date = new Date(timestamp)
  const hours = date.getHours().toString().padStart(2, '0')
  const minutes = date.getMinutes().toString().padStart(2, '0')
  const seconds = date.getSeconds().toString().padStart(2, '0')
  const ms = date.getMilliseconds().toString().padStart(3, '0')
  return `${hours}:${minutes}:${seconds}.${ms}`
}

const getStatusType = (status?: string) => {
  switch (status) {
    case 'Completed': return 'success'
    case 'Failed': return 'danger'
    case 'Running': return 'primary'
    case 'Pending': return 'warning'
    case 'skipped': return 'info'
    default: return 'info'
  }
}

onUnmounted(() => {
  if (isResizing.value) {
    stopResize()
  }
})
</script>

<template>
  <div 
    ref="panelRef"
    class="execution-panel"
    :class="{ 
      'is-open': isOpen,
      'is-resizing': isResizing 
    }"
    :style="{ height: panelHeight }"
  >
    <div 
      v-if="isOpen"
      class="resize-handle"
      @mousedown="startResize"
    >
      <div class="handle-bar"></div>
    </div>
    
    <div class="panel-header" @dblclick="togglePanel">
      <div class="header-left">
        <ElButton
          :icon="isOpen ? ArrowDown : ArrowUp"
          circle
          size="small"
          text
          @click="togglePanel"
        />
        
        <span class="header-title">Execution Results</span>
        
        <div v-if="executionStore.isExecuting" class="execution-indicator">
          <span class="execution-dot"></span>
          <span class="execution-text">Executing...</span>
        </div>
        
        <div v-if="executionSummary && !executionStore.isExecuting" class="summary-tags">
          <ElTag v-if="executionSummary.success > 0" type="success" size="small">
            ✅ {{ executionSummary.success }}
          </ElTag>
          <ElTag v-if="executionSummary.failed > 0" type="danger" size="small">
            ❌ {{ executionSummary.failed }}
          </ElTag>
          <ElTag v-if="executionSummary.skipped > 0" type="info" size="small">
            ⏭️ {{ executionSummary.skipped }}
          </ElTag>
          <ElTag v-if="executionSummary.totalTime" type="warning" size="small">
            ⏱️ {{ (executionSummary.totalTime / 1000).toFixed(2) }}s
          </ElTag>
        </div>
      </div>
      
      <div class="header-right">
        <span class="keyboard-hint">Ctrl+J to toggle • Click node to view result</span>
        <ElButton
          :icon="Close"
          circle
          size="small"
          text
          @click="closePanel"
        />
      </div>
    </div>
    
    <div v-if="isOpen" class="panel-body">
      <div v-if="!hasResults" class="empty-state">
        <ElEmpty description="Execute workflow to see results here">
          <template #image>
            <div class="empty-icon">📊</div>
          </template>
        </ElEmpty>
      </div>
      
      <div v-else-if="selectedResult" class="result-container">
        <ElCard class="result-card">
          <template #header>
            <div class="card-header">
              <div class="node-info">
                <span class="node-id">{{ selectedResult.nodeId }}</span>
                <ElTag :type="getStatusType(selectedResult.status)" size="small">
                  {{ selectedResult.status }}
                </ElTag>
              </div>
              <ElButton
                v-if="selectedResult.output"
                :icon="CopyDocument"
                size="small"
                text
                @click="copyToClipboard(formatJson(selectedResult.output))"
              >
                Copy Output
              </ElButton>
            </div>
          </template>
          
          <ElDescriptions :column="3" border size="small">
            <ElDescriptionsItem label="Status">
              <ElTag :type="getStatusType(selectedResult.status)" size="small">
                {{ selectedResult.status }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem label="Start Time">
              {{ formatTimestamp(selectedResult.startTime) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem label="End Time">
              {{ formatTimestamp(selectedResult.endTime) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem label="Duration" :span="3">
              <span v-if="selectedResult.executionTime" class="duration">
                {{ selectedResult.executionTime }}ms
              </span>
              <span v-else>-</span>
            </ElDescriptionsItem>
          </ElDescriptions>
          
          <div class="result-content">
            <ElCollapse 
              v-if="selectedResult.input || selectedResult.output || selectedResult.error" 
              :model-value="['input', 'output', 'error']"
            >
              <ElCollapseItem v-if="selectedResult.input" title="Input" name="input">
                <div class="data-section">
                  <ElButton
                    :icon="CopyDocument"
                    size="small"
                    text
                    class="copy-btn"
                    @click="copyToClipboard(formatJson(selectedResult.input))"
                  >
                    Copy
                  </ElButton>
                  <pre class="json-content">{{ formatJson(selectedResult.input) }}</pre>
                </div>
              </ElCollapseItem>
              
              <ElCollapseItem v-if="selectedResult.output" title="Output" name="output">
                <div class="data-section">
                  <ElButton
                    :icon="CopyDocument"
                    size="small"
                    text
                    class="copy-btn"
                    @click="copyToClipboard(formatJson(selectedResult.output))"
                  >
                    Copy
                  </ElButton>
                  <pre class="json-content">{{ formatJson(selectedResult.output) }}</pre>
                </div>
              </ElCollapseItem>
              
              <ElCollapseItem v-if="selectedResult.error" title="Error" name="error">
                <ElAlert type="error" :closable="false" show-icon>
                  <pre class="error-content">{{ selectedResult.error }}</pre>
                </ElAlert>
              </ElCollapseItem>
              
              <ElCollapseItem v-if="selectedResult.logs?.length" title="Logs" name="logs">
                <div v-for="(log, index) in selectedResult.logs" :key="index" class="log-entry">
                  {{ log }}
                </div>
              </ElCollapseItem>
            </ElCollapse>
            
            <div v-else-if="selectedResult.status === 'Pending' || selectedResult.status === 'Running'" class="status-message">
              <ElTag :type="getStatusType(selectedResult.status)" effect="plain">
                {{ selectedResult.status === 'Pending' ? 'Waiting' : 'Running...' }}
              </ElTag>
            </div>
          </div>
        </ElCard>
      </div>
      
      <div v-else class="selection-prompt">
        <ElEmpty description="Click on a node to view its execution result">
          <template #image>
            <div class="prompt-icon">👆</div>
          </template>
        </ElEmpty>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.execution-panel {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: var(--rf-color-bg-container);
  border-top: 1px solid var(--rf-color-border-base);
  box-shadow: var(--rf-shadow-panel);
  transition: height 0.3s ease;
  z-index: 50;
  display: flex;
  flex-direction: column;
}

.execution-panel.is-resizing {
  transition: none;
}

.resize-handle {
  position: absolute;
  top: -3px;
  left: 0;
  right: 0;
  height: 6px;
  cursor: ns-resize;
  z-index: 10;
}

.resize-handle:hover .handle-bar,
.is-resizing .handle-bar {
  opacity: 1;
}

.handle-bar {
  position: absolute;
  top: 2px;
  left: 50%;
  transform: translateX(-50%);
  width: 40px;
  height: 3px;
  background: var(--rf-color-border-lighter);
  border-radius: 2px;
  opacity: 0;
  transition: opacity 0.2s;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: var(--rf-color-bg-secondary);
  border-bottom: 1px solid var(--rf-color-border-base);
  cursor: pointer;
  user-select: none;
  flex-shrink: 0;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--rf-color-text-primary);
}

.summary-tags {
  display: flex;
  gap: 8px;
}

.execution-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
}

.execution-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--rf-color-info);
  animation: pulse-dot 1.5s infinite;
}

.execution-text {
  font-size: 12px;
  color: var(--rf-color-info);
  font-weight: 500;
}

// Pulsing animation for execution indicator - visual feedback that workflow is running
@keyframes pulse-dot {
  0%, 100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.5;
    transform: scale(1.2);
  }
}

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.keyboard-hint {
  font-size: 12px;
  color: var(--rf-color-text-secondary);
}

.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.empty-state,
.selection-prompt {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.empty-icon,
.prompt-icon {
  font-size: 48px;
  opacity: 0.8;
}

.result-container {
  height: 100%;
}

.result-card {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.node-info {
  display: flex;
  align-items: center;
  gap: 12px;
}

.node-id {
  font-size: 16px;
  font-weight: 600;
  color: var(--rf-color-text-primary);
}

.duration {
  color: var(--rf-color-info);
  font-weight: 600;
}

.result-content {
  margin-top: 16px;
  flex: 1;
  overflow-y: auto;
}

.json-content {
  margin: 0;
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.5;
  color: var(--rf-color-text-regular);
  white-space: pre-wrap;
  word-break: break-all;
  background: var(--rf-color-bg-secondary);
  padding: 12px;
  border-radius: 4px;
}

.error-content {
  margin: 0;
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

.log-entry {
  padding: 4px 8px;
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.5;
  color: var(--rf-color-text-regular);
  border-bottom: 1px solid var(--rf-color-border-base);
}

.log-entry:last-child {
  border-bottom: none;
}

.data-section {
  position: relative;
}

.copy-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 1;
}

.status-message {
  padding: 20px;
  text-align: center;
}

:deep(.el-card__body) {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>