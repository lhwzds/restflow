<script setup lang="ts">
import { Background } from '@vue-flow/background'
import { ControlButton, Controls } from '@vue-flow/controls'
import type { Connection, Edge } from '@vue-flow/core'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { computed, ref } from 'vue'
import { useWorkflowStore } from '../stores/workflowStore'
import Icon from './Icon.vue'
import AgentNode from './nodes/AgentNode.vue'
import HttpNode from './nodes/HttpNode.vue'
import ManualTriggerNode from './nodes/ManualTriggerNode.vue'
import NodeToolbar from './NodeToolbar.vue'
// Use Pinia store
const workflowStore = useWorkflowStore()

// Use store state as refs
const nodes = computed({
  get: () => workflowStore.nodes,
  set: (value) => {
    workflowStore.nodes = value
  },
})

const edges = computed({
  get: () => workflowStore.edges,
  set: (value) => {
    workflowStore.edges = value
  },
})

const isExecuting = computed(() => workflowStore.isExecuting)

// Use VueFlow hooks for interaction
const {
  project,
  vueFlowRef,
  addNodes,
  addEdges,
  onConnect,
  onPaneContextMenu,
  onNodeContextMenu,
  removeNodes,
  removeEdges,
  setViewport,
  toObject,
} = useVueFlow()

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

// Handle drag and drop
const handleDrop = (event: DragEvent) => {
  event.preventDefault()

  const data = event.dataTransfer?.getData('application/vueflow')
  if (!data) return

  const template = JSON.parse(data)
  const position = project({
    x: event.clientX - vueFlowRef.value!.getBoundingClientRect().left,
    y: event.clientY - vueFlowRef.value!.getBoundingClientRect().top,
  })

  addNodeAtPosition(template, position)
}

const handleDragOver = (event: DragEvent) => {
  event.preventDefault()
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'move'
  }
}

// Add node at specific position
const addNodeAtPosition = (template: any, position: { x: number; y: number }) => {
  const newNode = workflowStore.createNode(template, position)
  // Also add to VueFlow for visual update
  addNodes([newNode])
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

// Context menu state
const contextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  nodeId: null as string | null,
})

// Canvas context menu
onPaneContextMenu((event: MouseEvent) => {
  event.preventDefault()
  contextMenu.value = {
    show: true,
    x: event.clientX,
    y: event.clientY,
    nodeId: null,
  }
})

// Node context menu
onNodeContextMenu(({ event, node }) => {
  event.preventDefault()

  // Handle both mouse and touch events
  const x = 'clientX' in event ? event.clientX : (event as TouchEvent).touches[0]?.clientX || 0
  const y = 'clientY' in event ? event.clientY : (event as TouchEvent).touches[0]?.clientY || 0

  contextMenu.value = {
    show: true,
    x,
    y,
    nodeId: node.id,
  }
})

// Delete node
const deleteNode = () => {
  if (contextMenu.value.nodeId) {
    workflowStore.removeNode(contextMenu.value.nodeId)
    // Also remove from VueFlow
    removeNodes([contextMenu.value.nodeId])
  }
  contextMenu.value.show = false
}

// Clear canvas
const clearCanvas = () => {
  workflowStore.clearCanvas()
  contextMenu.value.show = false
}

// Close context menu when clicking elsewhere
const handlePaneClick = () => {
  contextMenu.value.show = false
}

// Execute workflow
const executeWorkflow = async () => {
  if (!workflowStore.hasNodes) {
    alert('Add some nodes first!')
    return
  }

  try {
    const result = await workflowStore.executeWorkflow()
    console.log('Execution result:', result)
    alert('Workflow execution success!')
  } catch (error) {
    console.error('Execution failed:', error)
    alert(`Workflow execution failed!`)
  }
}

const dark = ref(false)

function toggleDarkMode() {
  dark.value = !dark.value
}
function resetTransform() {
  setViewport({ x: 0, y: 0, zoom: 1 })
}
function logToObject() {
  console.log(toObject())
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
      :nodes="nodes"
      :edges="edges"
      class="basic-flow"
      :class="{ dark }"
      :default-viewport="{ zoom: 1.5 }"
      :min-zoom="0.2"
      :max-zoom="4"
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

        <ControlButton title="Toggle Dark Mode" @click="toggleDarkMode">
          <Icon v-if="dark" name="sun" />
          <Icon v-else name="moon" />
        </ControlButton>

        <ControlButton title="Log `toObject`" @click="logToObject">
          <Icon name="log" />
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
      <div v-if="contextMenu.nodeId" class="menu-item" @click="deleteNode">Delete Node</div>
      <div class="menu-item" @click="clearCanvas">Clear Canvas</div>
    </div>
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

.basic-flow.dark {
  background: #2d3748;
  color: #fffffb;
}

.basic-flow.dark .vue-flow__node {
  background: #4a5568;
  color: #fffffb;
}

.basic-flow.dark .vue-flow__node.selected {
  background: #333;
  box-shadow: 0 0 0 2px #2563eb;
}

.basic-flow .vue-flow__controls {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
}

.basic-flow.dark .vue-flow__controls {
  border: 1px solid #fffffb;
  background: #2d3748;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button {
  border: none;
  border-right: 1px solid #eee;
}

.basic-flow .vue-flow__controls .vue-flow__controls-button svg {
  height: 100%;
  width: 100%;
}

.basic-flow.dark .vue-flow__controls .vue-flow__controls-button {
  background: #333;
  fill: #fffffb;
  border: none;
  color: #fffffb;
}

.basic-flow.dark .vue-flow__controls .vue-flow__controls-button:hover {
  background: #4d4d4d;
}

.basic-flow.dark .vue-flow__controls .vue-flow__controls-button:last-child {
  border-right: none;
}

.basic-flow.dark .vue-flow__edge-textbg {
  fill: #292524;
}

.basic-flow.dark .vue-flow__edge-text {
  fill: #fffffb;
}
</style>
