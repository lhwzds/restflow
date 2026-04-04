<script setup lang="ts">
/**
 * Dialog for converting a chat session into a task.
 */
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { useTaskStore } from '@/stores/taskStore'
import { useToast } from '@/composables/useToast'

const props = defineProps<{
  open: boolean
  sessionId: string
  sessionName: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const { t } = useI18n()
const toast = useToast()
const taskStore = useTaskStore()

const taskName = ref('')
const taskInput = ref('')
const runImmediately = ref(false)
const isSubmitting = ref(false)

watch(
  () => props.open,
  (open) => {
    if (open) {
      taskName.value = `Task: ${props.sessionName}`
      taskInput.value = ''
      runImmediately.value = false
    }
  },
)

async function submit() {
  if (!taskName.value.trim()) return
  isSubmitting.value = true
  try {
    const result = await taskStore.convertSessionToTask({
      session_id: props.sessionId,
      name: taskName.value.trim(),
      input: taskInput.value.trim() || undefined,
      run_now: runImmediately.value,
    })
    if (result) {
      toast.success(t('workspace.session.convertSuccess'))
      emit('update:open', false)
    } else if (taskStore.error) {
      toast.error(taskStore.error || t('workspace.session.convertFailed'))
    }
  } finally {
    isSubmitting.value = false
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
      <DialogContent class="max-w-[28rem]">
        <DialogHeader>
        <DialogTitle>{{ t('workspace.session.convertToTask') }}</DialogTitle>
        <DialogDescription>{{ t('workspace.session.convertDescription') }}</DialogDescription>
      </DialogHeader>
      <div class="space-y-4">
        <div class="space-y-2">
          <Label>{{ t('workspace.session.taskName') }}</Label>
          <Input v-model="taskName" />
        </div>
        <div class="space-y-2">
          <Label>{{ t('workspace.session.inputOverride') }}</Label>
          <Textarea
            v-model="taskInput"
            :placeholder="t('workspace.session.inputOverridePlaceholder')"
            :rows="3"
          />
        </div>
        <label class="flex items-center gap-2 cursor-pointer">
          <input v-model="runImmediately" type="checkbox" class="rounded" />
          <span class="text-sm">{{ t('workspace.session.runImmediately') }}</span>
        </label>
      </div>
      <DialogFooter>
        <Button variant="outline" @click="emit('update:open', false)">
          {{ t('common.cancel') }}
        </Button>
        <Button :disabled="!taskName.trim() || isSubmitting" @click="submit">
          {{ t('workspace.session.convert') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
