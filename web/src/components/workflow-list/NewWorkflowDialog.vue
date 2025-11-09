<script setup lang="ts">
import { ElButton, ElDialog, ElForm, ElFormItem, ElInput, ElMessage } from 'element-plus'
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { VALIDATION_MESSAGES } from '@/constants'
import { createWorkflow } from '@/api/workflows'
import type { Workflow } from '@/types/generated/Workflow'

interface Props {
  visible: boolean
}

const props = defineProps<Props>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
}>()

const router = useRouter()
const workflowName = ref('')

const dialogVisible = ref(props.visible)

watch(() => props.visible, (newVal) => {
  dialogVisible.value = newVal
  if (newVal) {
    workflowName.value = ''
  }
})

watch(dialogVisible, (newVal) => {
  emit('update:visible', newVal)
})

async function handleCreate() {
  if (!workflowName.value?.trim()) {
    ElMessage.error(VALIDATION_MESSAGES.ENTER_WORKFLOW_NAME)
    return
  }

  try {
    // Create minimal workflow object
    const newWorkflow: Workflow = {
      id: `workflow-${Date.now()}-${Math.random().toString(36).substring(2, 11)}`,
      name: workflowName.value.trim(),
      nodes: [],
      edges: []
    }

    // Save to database immediately
    const result = await createWorkflow(newWorkflow)

    // Navigate to editor with the saved workflow ID
    router.push(`/workflow/${result.id}`)
    dialogVisible.value = false

    ElMessage.success('Workflow created successfully')
  } catch (error) {
    ElMessage.error('Failed to create workflow')
    console.error('Failed to create workflow:', error)
  }
}

function handleCancel() {
  dialogVisible.value = false
}
</script>

<template>
  <ElDialog 
    v-model="dialogVisible" 
    title="Create New Workflow" 
    width="500px"
    :close-on-click-modal="false"
  >
    <ElForm label-width="80px" @submit.prevent>
      <ElFormItem label="Name" required>
        <ElInput
          v-model="workflowName"
          placeholder="Enter workflow name"
          @keyup.enter="handleCreate"
        />
      </ElFormItem>
    </ElForm>
    
    <template #footer>
      <ElButton @click="handleCancel">Cancel</ElButton>
      <ElButton type="primary" @click="handleCreate">Create</ElButton>
    </template>
  </ElDialog>
</template>