<script setup lang="ts">
import { Background } from '@vue-flow/background'
import { ControlButton, Controls } from '@vue-flow/controls'
import type { Connection, Edge } from '@vue-flow/core'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { ref } from 'vue'
import { useDragAndDrop } from '../composables/node/useDragAndDrop'
import { useEdgeOperations } from '../composables/node/useEdgeOperations'
import { useNodeOperations } from '../composables/node/useNodeOperations'
import { useContextMenu } from '../composables/ui/useContextMenu'
import { useVueFlowHandlers } from '../composables/editor/useVueFlowHandlers'
import { useAsyncWorkflowExecution } from '../composables/execution/useAsyncWorkflowExecution'
import { AgentNode, HttpNode, ManualTriggerNode } from '../nodes'
import { useExecutionStore } from '../stores/executionStore'
import Icon from './Icon.vue'
import NodeConfigPanel from './NodeConfigPanel.vue'
import NodeToolbar from './NodeToolbar.vue'
import ExecutionPanel from './ExecutionPanel.vue'

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
  executionStore.openPanel()
  await startAsyncExecution()
}

function resetTransform() {
  setViewport({ x: 0, y: 0, zoom: 1 })
}
</script>

<template>
  <div class="workflow-editor" @click="handlePaneClick">
    <!-- Main canvas area -->
    <div class="canvas-container" :class="{ 'with-panel': executionStore.panelState.isOpen }">
      <!-- Node Toolbar -->
      <NodeToolbar @add-node="handleAddNode" />

    <!-- Execute Button -->
    <button class="execute-button" @click="executeWorkflow" :disabled="isExecuting">
      {{ isExecuting ? 'Executing...' : '▶️ Execute Workflow' }}
    </button>

    <!-- Workflow Canvas -->
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

      <Background pattern-color="#aaa" :gap="16" />
      <MiniMap />

      <Controls position="bottom-right">
        <ControlButton title="Reset Transform" @click="resetTransform">
          <Icon name="reset" />
        </ControlButton>
      </Controls>

      <!-- Manual Trigger Node Template -->
      <template #node-ManualTrigger="manualTriggerNodeProps">
        <ManualTriggerNode v-bind="manualTriggerNodeProps" />
      </template>

      <!-- Agent Node Template -->
      <template #node-Agent="agentNodeProps">
        <AgentNode v-bind="agentNodeProps" />
      </template>

      <!-- HTTP Node Template -->
      <template #node-HttpRequest="httpNodeProps">
        <HttpNode v-bind="httpNodeProps" />
      </template>
    </VueFlow>

    <!-- Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
    >
      <div v-if="contextMenu.nodeId" class="menu-item" @click="handleDeleteNode">Delete Node</div>
      <div class="menu-item" @click="handleClearCanvas">Clear Canvas</div>
    </div>

    <!-- Node Configuration Panel -->
    <NodeConfigPanel
      :node="selectedNode"
      @update="(node: any) => updateNodeData(node.id, node.data)"
      @close="closeConfigPanel"
    />
    
    <!-- Execution Results Panel (inside canvas container) -->
    <ExecutionPanel />
    </div>
  </div>
</template>

<style scoped>
.workflow-editor {
  width: 100%;
  height: 100%;
  position: relative;
}

.canvas-container {
  width: 100%;
  height: 100%;
  position: relative;
  overflow: hidden;
}

.context-menu {
  position: fixed;
  background: white;
  border-radius: 6px;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2);
  padding: 4px;
  z-index: 1000;
  min-width: 120px;
}

.menu-item {
  padding: 8px 12px;
  cursor: pointer;
  border-radius: 4px;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
}

.menu-item:hover {
  background: #8d8b8b;
}

.execute-button {
  position: absolute;
  bottom: 20px;
  left: 50%;
  transform: translateX(-50%);
  background: linear-gradient(135deg, #48bb78 0%, #38a169 100%);
  color: white;
  border: none;
  border-radius: 8px;
  padding: 12px 24px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
  z-index: 60;
  transition: all 0.2s;
}

.execute-button:hover:not(:disabled) {
  transform: translateX(-50%) translateY(-2px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.execute-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.vue-flow__minimap {
  bottom: 35px;
}

.basic-flow .vue-flow__controls {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button {
  border: none;
  border-right: 1px solid #eee;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button svg {
  height: 100%;
  width: 100%;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button:last-child {
  border-right: none;
}
</style>
