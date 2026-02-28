<script setup lang="ts">
/**
 * Dialog for creating a new agent.
 */
import { ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { createAgent } from '@/api/agents'
import { useModelsStore } from '@/stores/modelsStore'
import { useToast } from '@/composables/useToast'
import type { AIModel } from '@/types/generated/AIModel'

const props = defineProps<{ open: boolean }>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  created: [agent: { id: string; name: string; model: string }]
}>()

const { t } = useI18n()
const toast = useToast()
const modelsStore = useModelsStore()

const name = ref('')
const model = ref('')
const isSubmitting = ref(false)

const models = computed(() => modelsStore.getAllModels)

function generateDefaultAgentName(): string {
  const timestamp = new Date()
    .toISOString()
    .replace(/[-:TZ.]/g, '')
    .slice(0, 14)
  return `Agent ${timestamp}`
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = ''
      model.value = ''
    }
  },
)

async function submit() {
  isSubmitting.value = true
  try {
    const selectedModel = model.value.trim()
    const resolvedName = name.value.trim() || generateDefaultAgentName()
    const agent = await createAgent({
      name: resolvedName,
      agent: selectedModel ? { model: selectedModel as AIModel } : {},
    })
    toast.success(t('workspace.agent.createSuccess'))
    const emittedModel = agent.agent.model || selectedModel || models.value[0]?.model || 'gpt-5'
    emit('created', {
      id: agent.id,
      name: agent.name,
      model: emittedModel,
    })
    emit('update:open', false)
  } catch {
    toast.error(t('workspace.agent.createFailed'))
  } finally {
    isSubmitting.value = false
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="max-w-[24rem]">
      <DialogHeader>
        <DialogTitle>{{ t('workspace.agent.create') }}</DialogTitle>
      </DialogHeader>
      <div class="space-y-4">
        <div class="space-y-2">
          <Label>
            {{ t('workspace.agent.nameLabel') }} ({{ t('workspace.agent.optional') }})
          </Label>
          <Input
            v-model="name"
            :placeholder="t('workspace.agent.namePlaceholder')"
            @keydown.enter="submit"
          />
        </div>
        <div class="space-y-2">
          <Label>
            {{ t('workspace.agent.modelLabel') }} ({{ t('workspace.agent.optional') }})
          </Label>
          <Select v-model="model">
            <SelectTrigger>
              <SelectValue :placeholder="t('workspace.agent.modelPlaceholder')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="m in models" :key="m.model" :value="m.model">
                {{ m.name }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      <DialogFooter>
        <Button variant="outline" @click="emit('update:open', false)">
          {{ t('common.cancel') }}
        </Button>
        <Button :disabled="isSubmitting" @click="submit">
          {{ t('workspace.agent.createButton') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
