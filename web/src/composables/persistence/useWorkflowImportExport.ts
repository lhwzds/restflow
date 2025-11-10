import { ElMessage } from 'element-plus'
import { ref } from 'vue'
import { useWorkflowStore } from '../../stores/workflowStore'
import { useWorkflowConverter } from '../editor/useWorkflowConverter'
import { SUCCESS_MESSAGES, ERROR_MESSAGES, VALIDATION_MESSAGES } from '@/constants'

export interface ImportExportOptions {
  onImportSuccess?: (data: { name: string; nodes: any[]; edges: any[] }) => void
  onExportSuccess?: () => void
  onError?: (error: Error) => void
}

export function useWorkflowImportExport(options: ImportExportOptions = {}) {
  const workflowStore = useWorkflowStore()
  const isImporting = ref(false)
  const isExporting = ref(false)

  /**
   * Export workflow to JSON file
   */
  const exportWorkflow = (name: string) => {
    let url: string | null = null
    try {
      isExporting.value = true

      const data = {
        name,
        nodes: workflowStore.nodes,
        edges: workflowStore.edges,
        exportedAt: new Date().toISOString(),
        version: '1.0.0', // Add version for future compatibility
      }

      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
      url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `${name.replace(/\s+/g, '-').toLowerCase()}.json`
      link.click()

      ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_EXPORTED)
      options.onExportSuccess?.()
    } catch (error) {
      const err =
        error instanceof Error ? error : new Error(ERROR_MESSAGES.FAILED_TO_EXPORT('workflow'))
      ElMessage.error(err.message)
      options.onError?.(err)
    } finally {
      // Always revoke the URL to prevent memory leaks
      if (url) {
        URL.revokeObjectURL(url)
      }
      isExporting.value = false
    }
  }

  /**
   * Import workflow from JSON file
   */
  const importWorkflow = (): Promise<void> => {
    return new Promise((resolve, reject) => {
      const input = document.createElement('input')
      input.type = 'file'
      input.accept = '.json'

      input.onchange = async (event: Event) => {
        const file = (event.target as HTMLInputElement).files?.[0]
        if (!file) {
          resolve()
          return
        }

        isImporting.value = true
        try {
          const text = await file.text()
          const data = JSON.parse(text)

          // Validate imported data
          if (!data.nodes || !Array.isArray(data.nodes)) {
            throw new Error('Invalid workflow file: missing nodes')
          }
          if (!data.edges || !Array.isArray(data.edges)) {
            throw new Error('Invalid workflow file: missing edges')
          }

          // Convert from backend format (our standard export format)
          const { convertFromBackendFormat } = useWorkflowConverter()
          const { nodes, edges } = convertFromBackendFormat(data)

          // Update store
          workflowStore.loadWorkflow(nodes, edges)

          ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_IMPORTED)
          options.onImportSuccess?.(data)
          resolve()
        } catch (error) {
          const err = error instanceof Error ? error : new Error('Failed to import workflow')

          // Provide more specific error messages
          if (err.message.includes('JSON')) {
            ElMessage.error(VALIDATION_MESSAGES.INVALID_JSON)
          } else if (err.message.includes('Invalid workflow file')) {
            ElMessage.error(err.message)
          } else {
            ElMessage.error(ERROR_MESSAGES.FAILED_TO_IMPORT('workflow') + ': ' + err.message)
          }

          options.onError?.(err)
          reject(err)
        } finally {
          isImporting.value = false
        }
      }

      // Handle cancel
      input.oncancel = () => {
        resolve()
      }

      input.click()
    })
  }

  /**
   * Import workflow from drag and drop
   */
  const importFromDrop = async (file: File): Promise<void> => {
    if (!file.name.endsWith('.json')) {
      ElMessage.error(VALIDATION_MESSAGES.INVALID_FORMAT('file'))
      return
    }

    isImporting.value = true
    try {
      const text = await file.text()
      const data = JSON.parse(text)

      if (!data.nodes || !data.edges) {
        throw new Error('Invalid workflow file format')
      }

      const { convertFromBackendFormat } = useWorkflowConverter()
      const { nodes, edges } = convertFromBackendFormat(data)
      workflowStore.loadWorkflow(nodes, edges)

      ElMessage.success(SUCCESS_MESSAGES.WORKFLOW_IMPORTED)
      options.onImportSuccess?.(data)
    } catch (error) {
      const err =
        error instanceof Error ? error : new Error(ERROR_MESSAGES.FAILED_TO_IMPORT('workflow'))
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_IMPORT('workflow') + ': ' + err.message)
      options.onError?.(err)
      throw err
    } finally {
      isImporting.value = false
    }
  }

  /**
   * Export workflow to clipboard
   */
  const copyToClipboard = async (name: string) => {
    try {
      const data = {
        name,
        nodes: workflowStore.nodes,
        edges: workflowStore.edges,
        exportedAt: new Date().toISOString(),
        version: '1.0.0',
      }

      await navigator.clipboard.writeText(JSON.stringify(data, null, 2))
      ElMessage.success(SUCCESS_MESSAGES.COPIED)
    } catch (error) {
      ElMessage.error(ERROR_MESSAGES.FAILED_TO_CREATE('clipboard copy'))
      const err = error instanceof Error ? error : new Error('Failed to copy to clipboard')
      options.onError?.(err)
    }
  }

  /**
   * Import workflow from clipboard
   */
  const pasteFromClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText()
      const data = JSON.parse(text)

      if (!data.nodes || !data.edges) {
        throw new Error('Invalid workflow data in clipboard')
      }

      const { convertFromBackendFormat } = useWorkflowConverter()
      const { nodes, edges } = convertFromBackendFormat(data)
      workflowStore.loadWorkflow(nodes, edges)

      ElMessage.success(SUCCESS_MESSAGES.IMPORTED('Workflow'))
      options.onImportSuccess?.(data)
    } catch (error) {
      if (error instanceof SyntaxError) {
        ElMessage.error(VALIDATION_MESSAGES.INVALID_FORMAT('clipboard data'))
      } else {
        ElMessage.error(ERROR_MESSAGES.FAILED_TO_IMPORT('workflow from clipboard'))
      }
      const err = error instanceof Error ? error : new Error('Failed to paste from clipboard')
      options.onError?.(err)
    }
  }

  return {
    isImporting,
    isExporting,
    exportWorkflow,
    importWorkflow,
    importFromDrop,
    copyToClipboard,
    pasteFromClipboard,
  }
}
