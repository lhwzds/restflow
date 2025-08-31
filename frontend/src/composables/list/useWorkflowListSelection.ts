import { ElMessage } from 'element-plus'
import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import type { Workflow } from '@/types/generated/Workflow'
import { useWorkflowList } from './useWorkflowList'

export function useWorkflowListSelection(workflows: Ref<Workflow[]>) {
  const { duplicateWorkflow } = useWorkflowList()
  const selectedWorkflowId = ref<string | null>(null)
  const copiedWorkflow = ref<Workflow | null>(null)

  function selectWorkflow(workflowId: string) {
    selectedWorkflowId.value = selectedWorkflowId.value === workflowId ? null : workflowId
  }

  function clearSelection() {
    selectedWorkflowId.value = null
  }

  function copyWorkflow() {
    if (!selectedWorkflowId.value) return
    
    const workflow = workflows.value.find((w) => w.id === selectedWorkflowId.value)
    if (workflow) {
      copiedWorkflow.value = workflow
      ElMessage.success('Workflow copied to clipboard')
    }
  }

  async function pasteWorkflow() {
    if (!copiedWorkflow.value) return
    
    await duplicateWorkflow(
      copiedWorkflow.value.id,
      `${copiedWorkflow.value.name} (Copy)`
    )
  }

  function handleKeyDown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === 'c') {
      copyWorkflow()
    }

    if ((event.ctrlKey || event.metaKey) && event.key === 'v') {
      pasteWorkflow()
    }

    if (event.key === 'Escape') {
      clearSelection()
    }
  }

  onMounted(() => {
    document.addEventListener('keydown', handleKeyDown)
  })

  onUnmounted(() => {
    document.removeEventListener('keydown', handleKeyDown)
  })

  return {
    selectedWorkflowId,
    copiedWorkflow,
    selectWorkflow,
    clearSelection,
    copyWorkflow,
    pasteWorkflow,
  }
}