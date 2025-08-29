import { defineStore } from 'pinia'
import type { TaskStatus } from '@/types/generated/TaskStatus'

export type NodeExecutionStatus = TaskStatus | 'skipped'

type Json = any

export interface NodeExecutionResult {
  nodeId: string
  status: NodeExecutionStatus
  startTime?: number
  endTime?: number
  executionTime?: number
  input?: Json
  output?: Json
  error?: string
  logs?: string[]
}

export interface ExecutionSummary {
  totalNodes: number
  success: number
  failed: number
  skipped: number
  running: number
  totalTime?: number
}

interface ExecutionState {
  // Current execution
  currentExecutionId: string | null
  isExecuting: boolean
  
  // Node results
  nodeResults: Map<string, NodeExecutionResult>
  
  // Panel state
  panelState: {
    isOpen: boolean
    height: number // percentage (0-100)
    selectedNodeId: string | null
    viewMode: 'details' | 'timeline' | 'logs'
  }
}

export const useExecutionStore = defineStore('execution', {
  state: (): ExecutionState => ({
    currentExecutionId: null,
    isExecuting: false,
    nodeResults: new Map(),
    panelState: {
      isOpen: false,
      height: 35, // 35% default height
      selectedNodeId: null,
      viewMode: 'details',
    },
  }),

  getters: {
    executionSummary(): ExecutionSummary | null {
      if (this.nodeResults.size === 0) return null

      const results = Array.from(this.nodeResults.values())
      const summary: ExecutionSummary = {
        totalNodes: results.length,
        success: results.filter(r => r.status === 'Completed').length,
        failed: results.filter(r => r.status === 'Failed').length,
        skipped: results.filter(r => r.status === 'skipped').length,
        running: results.filter(r => r.status === 'Running').length,
        totalTime: undefined,
      }

      // Calculate total execution time
      const completedResults = results.filter(r => r.startTime && r.endTime)
      if (completedResults.length > 0) {
        const minStart = Math.min(...completedResults.map(r => r.startTime!))
        const maxEnd = Math.max(...completedResults.map(r => r.endTime!))
        summary.totalTime = maxEnd - minStart
      }

      return summary
    },

    sortedNodeResults(): NodeExecutionResult[] {
      return Array.from(this.nodeResults.values()).sort((a, b) => {
        // Sort by start time, then by node ID
        if (a.startTime && b.startTime) {
          return a.startTime - b.startTime
        }
        return a.nodeId.localeCompare(b.nodeId)
      })
    },

    selectedNodeResult(): NodeExecutionResult | null {
      if (!this.panelState.selectedNodeId) return null
      return this.nodeResults.get(this.panelState.selectedNodeId) || null
    },

    hasResults(): boolean {
      return this.nodeResults.size > 0
    },
  },

  actions: {
    // Execution management
    startExecution(executionId: string) {
      this.currentExecutionId = executionId
      this.isExecuting = true
      this.nodeResults.clear()
      this.panelState.isOpen = true // Auto-open panel on execution
      this.panelState.selectedNodeId = null
    },

    endExecution() {
      this.isExecuting = false
      // Auto-select first error node if any
      const errorNode = this.sortedNodeResults.find(r => r.status === 'Failed')
      if (errorNode) {
        this.panelState.selectedNodeId = errorNode.nodeId
      }
    },

    clearExecution() {
      this.currentExecutionId = null
      this.isExecuting = false
      this.nodeResults.clear()
      this.panelState.selectedNodeId = null
    },

    // Node results management
    setNodeResult(nodeId: string, result: Partial<NodeExecutionResult>) {
      const existing = this.nodeResults.get(nodeId) || { nodeId, status: 'Pending' as NodeExecutionStatus }
      const updated = { ...existing, ...result }
      
      // Preserve startTime if not provided
      if (!updated.startTime && 'startTime' in existing) {
        updated.startTime = existing.startTime
      }
      
      // Calculate execution time if both timestamps exist
      if (updated.startTime && updated.endTime) {
        updated.executionTime = updated.endTime - updated.startTime
      }
      
      this.nodeResults.set(nodeId, updated as NodeExecutionResult)
    },

    updateNodeStatus(nodeId: string, status: NodeExecutionStatus) {
      const result: NodeExecutionResult = this.nodeResults.get(nodeId) || { nodeId, status: 'Pending' as NodeExecutionStatus }
      
      if (status === 'Running') {
        result.startTime = Date.now()
      } else if (status === 'Completed' || status === 'Failed') {
        result.endTime = Date.now()
      }
      
      result.status = status
      this.nodeResults.set(nodeId, result)
    },

    // Parse execution context and populate results
    parseExecutionContext(context: any) {
      if (!context) return

      // Parse node outputs from execution context
      // Handle both formats: node_outputs or results
      const outputs = context.node_outputs || context.results
      if (!outputs) return

      Object.entries(outputs).forEach(([nodeId, output]) => {
        // Check if node was skipped
        if (output && typeof output === 'object' && (output as any).skipped) {
          this.setNodeResult(nodeId, {
            nodeId,
            status: 'skipped' as const,
            output,
            endTime: Date.now(),
          })
        } else {
          this.setNodeResult(nodeId, {
            nodeId,
            status: 'Completed',
            output,
            endTime: Date.now(),
          })
        }
      })
    },

    // Panel state management
    togglePanel() {
      this.panelState.isOpen = !this.panelState.isOpen
      if (this.panelState.isOpen && !this.panelState.selectedNodeId && this.nodeResults.size > 0) {
        // Auto-select first node when opening panel
        this.panelState.selectedNodeId = this.sortedNodeResults[0]?.nodeId || null
      }
    },

    openPanel() {
      this.panelState.isOpen = true
    },

    closePanel() {
      this.panelState.isOpen = false
    },

    setPanelHeight(height: number) {
      // Clamp between 25% and 60%
      this.panelState.height = Math.min(60, Math.max(25, height))
    },

    selectNode(nodeId: string | null) {
      this.panelState.selectedNodeId = nodeId
    },

    setViewMode(mode: 'details' | 'timeline' | 'logs') {
      this.panelState.viewMode = mode
    },

    // Get node execution status for styling
    getNodeStatus(nodeId: string): NodeExecutionStatus | null {
      const result = this.nodeResults.get(nodeId)
      return result?.status || null
    },

    // Update from async execution tasks (the only way to update execution status)
    updateFromTasks(tasks: Array<{
      node_id: string
      status: NodeExecutionStatus
      input?: Json
      output?: Json
      error?: string | null
      started_at?: bigint | null
      completed_at?: bigint | null
    }>) {
      tasks.forEach(task => {
        this.setNodeResult(task.node_id, {
          nodeId: task.node_id,
          status: task.status,
          input: task.input,
          output: task.output,
          error: task.error || undefined,
          startTime: task.started_at ? Number(task.started_at) * 1000 : undefined,
          endTime: task.completed_at ? Number(task.completed_at) * 1000 : undefined,
        })
      })
    },
  },
})