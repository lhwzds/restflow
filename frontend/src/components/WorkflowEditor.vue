<script setup lang="ts">
import { Background } from '@vue-flow/background'
import { ControlButton, Controls } from '@vue-flow/controls'
import type { Connection, Edge } from '@vue-flow/core'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { storeToRefs } from 'pinia'
import { reactive, ref } from 'vue'
import { useDragAndDrop } from '../composables/useDragAndDrop'
import { useWorkflowStore } from '../stores/workflowStore'
import Icon from './Icon.vue'
import AgentNode from './nodes/AgentNode.vue'
import HttpNode from './nodes/HttpNode.vue'
import ManualTriggerNode from './nodes/ManualTriggerNode.vue'
import NodeConfigPanel from './NodeConfigPanel.vue'
import NodeToolbar from './NodeToolbar.vue'

// Use Pinia store and composables
const workflowStore = useWorkflowStore()
const { isExecuting } = storeToRefs(workflowStore)
const { handleDrop, handleDragOver } = useDragAndDrop()

// Use VueFlow hooks for interaction
const { onConnect, onPaneContextMenu, onNodeContextMenu, onNodeDoubleClick, setViewport, updateNode } =
  useVueFlow()

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
  workflowStore.addEdge(newEdge)
})

// Handle node double click to open config panel
onNodeDoubleClick(({ node }) => {
  selectedNode.value = node
})

// Handle node update from config panel
const handleNodeUpdate = (updatedNode: any) => {
  updateNode(updatedNode.id, updatedNode)
  workflowStore.updateNodeData(updatedNode.id, updatedNode.data)
}

// Close config panel
const closeConfigPanel = () => {
  selectedNode.value = null
}

// Add node at specific position (for toolbar clicks)
const addNodeAtPosition = (template: any, position: { x: number; y: number }) => {
  workflowStore.createNode(template, position)
}

// Handle toolbar click to add node
const handleAddNode = (template: any) => {
  // Add node at center of canvas with slight randomization
  const position = {
    x: 250 + Math.random() * 100,
    y: 150 + Math.random() * 100,
  }
  addNodeAtPosition(template, position)
}

// Context menu state - simplified
const contextMenu = reactive({
  show: false,
  x: 0,
  y: 0,
  nodeId: null as string | null,
})

// Canvas context menu
onPaneContextMenu((event: MouseEvent) => {
  event.preventDefault()
  Object.assign(contextMenu, {
    show: true,
    x: event.clientX,
    y: event.clientY,
    nodeId: null,
  })
})

// Node context menu
onNodeContextMenu(({ event, node }) => {
  event.preventDefault()
  const x = 'clientX' in event ? event.clientX : (event as TouchEvent).touches[0]?.clientX || 0
  const y = 'clientY' in event ? event.clientY : (event as TouchEvent).touches[0]?.clientY || 0

  Object.assign(contextMenu, {
    show: true,
    x,
    y,
    nodeId: node.id,
  })
})

// Handle delete node from context menu
const handleDeleteNode = () => {
  if (contextMenu.nodeId) {
    workflowStore.removeNode(contextMenu.nodeId)
  }
  contextMenu.show = false
}

// Handle clear canvas from context menu
const handleClearCanvas = () => {
  workflowStore.clearCanvas()
  contextMenu.show = false
}

// Close context menu when clicking elsewhere
const handlePaneClick = () => {
  contextMenu.show = false
}

// Execute workflow
const executeWorkflow = async () => {
  try {
    await workflowStore.executeWorkflow()
    alert('Workflow execution success!')
  } catch (error) {
    alert(`Workflow execution failed: ${workflowStore.executionError || 'Unknown error'}`)
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
      v-model:nodes="workflowStore.nodes"
      v-model:edges="workflowStore.edges"
      class="basic-flow"
      :default-viewport="{ zoom: 1.5 }"
      :min-zoom="0.2"
      :max-zoom="4"
      fit-view-on-init
      @drop="handleDrop"
      @dragover="handleDragOver"
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
      <template #node-manual-trigger="manualTriggerNodeProps">
        <ManualTriggerNode v-bind="manualTriggerNodeProps" />
      </template>

      <!-- Agent Node Template -->
      <template #node-agent="agentNodeProps">
        <AgentNode v-bind="agentNodeProps" />
      </template>

      <!-- HTTP Node Template -->
      <template #node-http="httpNodeProps">
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
      @update="handleNodeUpdate"
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
