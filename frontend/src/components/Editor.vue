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
import { useVueFlowHandlers } from '../composables/workflow/useVueFlowHandlers'
import { useWorkflowExecution } from '../composables/workflow/useWorkflowExecution'
import { AgentNode, HttpNode, ManualTriggerNode } from '../nodes'
import Icon from './Icon.vue'
import NodeConfigPanel from './NodeConfigPanel.vue'
import NodeToolbar from './NodeToolbar.vue'

// Use composables
const { handleDrop, handleDragOver } = useDragAndDrop()
const { nodes, createNode, updateNodePosition, deleteNode, clearAll, updateNodeData } =
  useNodeOperations()
const { edges, addEdge } = useEdgeOperations()
const { handleEdgesChange, handleNodesChange } = useVueFlowHandlers()
const { isExecuting, executeCurrentWorkflow } = useWorkflowExecution()

// Use VueFlow hooks for interaction
const {
  onConnect,
  onPaneContextMenu,
  onNodeContextMenu,
  onNodeDoubleClick,
  onNodeDragStop,
  setViewport,
} = useVueFlow()

// Selected node for configuration panel
const selectedNode = ref<any>(null)

// Handle connections between nodes
onConnect((connection: Connection) => {
  const newEdge: Edge = {
    id: `e${connection.source}->${connection.target}`,
    source: connection.source!,
    target: connection.target!,
    animated: true,
  }
  addEdge(newEdge)
})

// Handle node double click to open config panel
onNodeDoubleClick(({ node }) => {
  selectedNode.value = node
})

// Handle node drag stop to mark as dirty
onNodeDragStop(({ node }) => {
  updateNodePosition(node.id, node.position)
})

// Close config panel
const closeConfigPanel = () => {
  selectedNode.value = null
}

// Handle toolbar click to add node
const handleAddNode = (template: any) => {
  // Add node at center of canvas with slight randomization
  const position = {
    x: 250 + Math.random() * 100,
    y: 150 + Math.random() * 100,
  }
  createNode(template, position)
}

// Context menu management
const { state: contextMenu, show: showContextMenu, hide: hideContextMenu } = useContextMenu()

// Canvas context menu
onPaneContextMenu((event: MouseEvent) => showContextMenu(event))

// Node context menu
onNodeContextMenu(({ event, node }) => showContextMenu(event, node.id))

// Handle delete node from context menu
const handleDeleteNode = () => {
  if (contextMenu.nodeId) {
    deleteNode(contextMenu.nodeId)
  }
  hideContextMenu()
}

// Handle clear canvas from context menu
const handleClearCanvas = () => {
  clearAll()
  hideContextMenu()
}

// Close context menu when clicking elsewhere
const handlePaneClick = () => {
  hideContextMenu()
}

// Execute workflow
const executeWorkflow = async () => {
  const result = await executeCurrentWorkflow()
  if (result.success) {
    alert('Workflow execution success!')
  } else {
    alert(`Workflow execution failed: ${result.error}`)
  }
}

function resetTransform() {
  setViewport({ x: 0, y: 0, zoom: 1 })
}
</script>

<template>
  <div class="workflow-editor" @click="handlePaneClick">
    <!-- Node Toolbar -->
    <NodeToolbar @add-node="handleAddNode" />

    <!-- Execute Button -->
    <button class="execute-button" @click="executeWorkflow" :disabled="isExecuting">
      {{ isExecuting ? 'Executing...' : '▶️ Execute workflow' }}
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
      <!-- <Background /> -->

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
  </div>
</template>

<style scoped>
.workflow-editor {
  width: 100%;
  height: 100%;
  position: relative;
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
  z-index: 10;
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
