<script setup lang="ts">
import { Background } from '@vue-flow/background'
import { ControlButton, Controls } from '@vue-flow/controls'
import type { Connection, Edge } from '@vue-flow/core'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { ElTooltip } from 'element-plus'
import { Play } from 'lucide-vue-next'
import { ref } from 'vue'
import { useVueFlowHandlers } from '../../composables/editor/useVueFlowHandlers'
import { useAsyncWorkflowExecution } from '../../composables/execution/useAsyncWorkflowExecution'
import { useDragAndDrop } from '../../composables/node/useDragAndDrop'
import { useEdgeOperations } from '../../composables/node/useEdgeOperations'
import { useNodeOperations } from '../../composables/node/useNodeOperations'
import { useKeyboardShortcuts } from '../../composables/shared/useKeyboardShortcuts'
import { useContextMenu } from '../../composables/ui/useContextMenu'
import { AgentNode, HttpNode, ManualTriggerNode, WebhookTriggerNode } from '../../nodes'
import { useExecutionStore } from '../../stores/executionStore'
import ExecutionPanel from './ExecutionPanel.vue'
import Icon from '../shared/Icon.vue'
import NodeConfigPanel from './NodeConfigPanel.vue'
import NodeToolbar from './NodeToolbar.vue'

const { handleDrop, handleDragOver } = useDragAndDrop()
const { nodes, createNode, updateNodePosition, deleteNode, clearAll, updateNodeData } =
  useNodeOperations()
const { edges, addEdge } = useEdgeOperations()
const { handleEdgesChange, handleNodesChange } = useVueFlowHandlers()
const { isExecuting, startAsyncExecution } = useAsyncWorkflowExecution()
const executionStore = useExecutionStore()

const {
  onConnect,
  onPaneContextMenu,
  onNodeContextMenu,
  onNodeClick,
  onNodeDoubleClick,
  onNodeDragStop,
  setViewport,
} = useVueFlow()

const selectedNode = ref<any>(null)

onConnect((connection: Connection) => {
  const newEdge: Edge = {
    id: `e${connection.source}->${connection.target}`,
    source: connection.source!,
    target: connection.target!,
    animated: true,
  }
  addEdge(newEdge)
})

onNodeClick(({ node }) => {
  if (executionStore.hasResults) {
    executionStore.selectNode(node.id)
    if (!executionStore.panelState.isOpen) {
      executionStore.openPanel()
    }
  }
})

onNodeDoubleClick(({ node }) => {
  selectedNode.value = node
})

onNodeDragStop(({ node }) => {
  updateNodePosition(node.id, node.position)
})

const closeConfigPanel = () => {
  selectedNode.value = null
}

const handleAddNode = (template: { type: string; defaultData: any }) => {
  const position = {
    x: 250 + Math.random() * 100,
    y: 150 + Math.random() * 100,
  }
  createNode(template, position)
}

const { state: contextMenu, show: showContextMenu, hide: hideContextMenu } = useContextMenu()

onPaneContextMenu((event: MouseEvent) => showContextMenu(event))

onNodeContextMenu(({ event, node }) => showContextMenu(event, node.id))

const handleDeleteNode = () => {
  if (contextMenu.nodeId) {
    deleteNode(contextMenu.nodeId)
  }
  hideContextMenu()
}

const handleClearCanvas = () => {
  clearAll()
  hideContextMenu()
}

const handlePaneClick = () => {
  hideContextMenu()
}

const executeWorkflow = async () => {
  // Open panel first to show execution progress in real-time
  executionStore.openPanel()
  await startAsyncExecution()
}

function resetTransform() {
  setViewport({ x: 0, y: 0, zoom: 1 })
}

useKeyboardShortcuts({
  f5: () => {
    if (!isExecuting.value) {
      executeWorkflow()
    }
  },
})
</script>

<template>
  <div class="workflow-editor" @click="handlePaneClick">
    <div class="canvas-container" :class="{ 'with-panel': executionStore.panelState.isOpen }">
      <NodeToolbar @add-node="handleAddNode" />

      <ElTooltip content="Run the workflow (F5)" placement="top">
        <button class="execute-button" @click="executeWorkflow" :disabled="isExecuting">
          <Play :size="16" class="execute-icon" />
          {{ isExecuting ? 'Executing...' : 'Execute Workflow' }}
        </button>
      </ElTooltip>

      <VueFlow
        v-model:nodes="nodes"
        v-model:edges="edges"
        class="basic-flow"
        :default-viewport="{ zoom: 1.5 }"
        :min-zoom="0.2"
        :max-zoom="4"
        @drop="handleDrop"
        @dragover="handleDragOver"
        @edges-change="handleEdgesChange"
        @nodes-change="handleNodesChange"
      >
        <Background />

        <MiniMap position="bottom-right" />

        <Controls position="bottom-right" :style="{ bottom: '200px' }">
          <ControlButton @click="resetTransform">
            <Icon name="reset" />
          </ControlButton>
        </Controls>

        <template #node-ManualTrigger="manualTriggerNodeProps">
          <ManualTriggerNode v-bind="manualTriggerNodeProps" />
        </template>

        <template #node-WebhookTrigger="webhookTriggerNodeProps">
          <WebhookTriggerNode v-bind="webhookTriggerNodeProps" />
        </template>

        <template #node-Agent="agentNodeProps">
          <AgentNode v-bind="agentNodeProps" />
        </template>

        <template #node-HttpRequest="httpNodeProps">
          <HttpNode v-bind="httpNodeProps" />
        </template>
      </VueFlow>

      <div
        v-if="contextMenu.show"
        class="context-menu"
        :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
      >
        <div v-if="contextMenu.nodeId" class="menu-item" @click="handleDeleteNode">Delete Node</div>
        <div class="menu-item" @click="handleClearCanvas">Clear Canvas</div>
      </div>

      <NodeConfigPanel
        :node="selectedNode"
        @update="(node: any) => updateNodeData(node.id, node.data)"
        @close="closeConfigPanel"
      />

      <ExecutionPanel />
    </div>
  </div>
</template>

<style lang="scss" scoped>
.workflow-editor {
  width: 100%;
  height: 100%;
  position: relative;
  background-color: var(--rf-color-bg-page);
}

.canvas-container {
  width: 100%;
  height: 100%;
  position: relative;
  overflow: hidden;
  background-color: var(--rf-color-bg-page);

  :deep(.vue-flow__controls) {
    background: var(--rf-color-bg-container);
    box-shadow: var(--rf-shadow-base);
    border: 1px solid var(--rf-color-border-base);

    .vue-flow__controls-button,
    button[type='button'] {
      background: var(--rf-color-bg-container);
      border: 1px solid var(--rf-color-border-base);
      border-bottom: 2px solid transparent;
      color: var(--rf-color-text-regular);
      transition:
        border-bottom-color 0.2s,
        background 0.2s;

      &:hover:not(:disabled) {
        background: var(--rf-color-bg-secondary);
        border-bottom-color: var(--rf-color-primary);
        color: var(--rf-color-text-regular); // Keep text/icon color unchanged
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      svg {
        fill: currentColor;
      }
    }
  }
}

.context-menu {
  position: fixed;
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-lg);
  padding: var(--rf-spacing-xs);
  z-index: 1000;
  min-width: 120px;
}

.menu-item {
  padding: var(--rf-spacing-sm) var(--rf-spacing-md);
  cursor: pointer;
  border-radius: var(--rf-radius-small);
  font-size: var(--rf-font-size-sm);
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-sm);
}

.menu-item:hover {
  background: var(--rf-color-border-lighter);
}

.execute-button {
  position: absolute;
  bottom: var(--rf-spacing-xl);
  left: 50%;
  transform: translateX(-50%);
  background: var(--rf-gradient-success);
  color: var(--rf-color-white);
  border: none;
  border-radius: var(--rf-radius-large);
  padding: var(--rf-spacing-md) var(--rf-spacing-2xl);
  font-size: var(--rf-font-size-base);
  font-weight: var(--rf-font-weight-semibold);
  cursor: pointer;
  box-shadow: var(--rf-shadow-card);
  z-index: 60;
  transition: all var(--rf-transition-fast);
}

.execute-button:hover:not(:disabled) {
  transform: translateX(-50%) translateY(-2px);
  box-shadow: var(--rf-shadow-lg);
}

.execute-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.execute-icon {
  vertical-align: middle;
  margin-right: var(--rf-spacing-sm);
}

.vue-flow__minimap {
  bottom: 35px;
  background: var(--rf-color-bg-container);
  border: 1px solid var(--rf-color-border-base);
}

.basic-flow {
  background-color: var(--rf-color-bg-page);
}

.basic-flow .vue-flow__controls {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  background: var(--rf-color-bg-container);
  border: 1px solid var(--rf-color-border-base);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-base);
}

.basic-flow .vue-flow__controls .vue-flow__controls-button {
  border: none;
  border-right: 1px solid var(--rf-color-border-lighter);
  background: var(--rf-color-bg-container);
  color: var(--rf-color-text-regular);
}

.basic-flow .vue-flow__controls .vue-flow__controls-button:hover {
  background-color: var(--rf-color-primary-bg-lighter);
  color: var(--rf-color-primary);
}

.basic-flow .vue-flow__controls .vue-flow__controls-button svg {
  height: 100%;
  width: 100%;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button:last-child {
  border-right: none;
}
</style>
