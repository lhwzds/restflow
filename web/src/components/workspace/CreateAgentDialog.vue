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
import { BackendError } from '@/api/http-client'
import { useModelsStore } from '@/stores/modelsStore'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import type { ModelId } from '@/types/generated/ModelId'
import type { Provider } from '@/types/generated/Provider'
import type { WorkspaceAgentModelSelection } from '@/types/workspace'
import { getProviderDisplayName } from '@/utils/providerCatalog'
import {
  extractOperationAssessment,
  formatOperationAssessment,
} from '@/utils/operationAssessment'

const props = defineProps<{ open: boolean }>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  created: [agent: WorkspaceAgentModelSelection]
}>()

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()
const modelsStore = useModelsStore()

const name = ref('')
const provider = ref<Provider | ''>('')
const model = ref<ModelId | ''>('')
const isSubmitting = ref(false)

const providers = computed(() => modelsStore.getProviders)
const models = computed(() =>
  provider.value ? modelsStore.getModelsByProvider(provider.value as Provider) : [],
)

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
      const firstProvider = providers.value[0]
      provider.value = firstProvider ?? ''
      model.value = firstProvider
        ? (modelsStore.getFirstModelByProvider(firstProvider) ?? '')
        : ''
    }
  },
  { immediate: true },
)

watch(
  provider,
  (selectedProvider) => {
    if (!selectedProvider) {
      model.value = ''
      return
    }
    if (!model.value || !modelsStore.isModelInProvider(selectedProvider, model.value)) {
      model.value = modelsStore.getFirstModelByProvider(selectedProvider) ?? ''
    }
  },
  { immediate: true },
)

async function submit() {
  const selectedProvider = provider.value
  const selectedModel = model.value
  if (!selectedProvider || !selectedModel) {
    toast.error(t('workspace.agent.providerModelRequired'))
    return
  }

  isSubmitting.value = true
  try {
    const resolvedName = name.value.trim() || generateDefaultAgentName()
    const request = {
      name: resolvedName,
      agent: {
        model: selectedModel as ModelId,
        model_ref: {
          provider: selectedProvider as Provider,
          model: selectedModel as ModelId,
        },
      },
    }
    let agent
    try {
      agent = await createAgent(request)
    } catch (error) {
      const assessment = extractOperationAssessment(error)
      if (
        error instanceof BackendError &&
        error.code === 428 &&
        assessment?.confirmation_token
      ) {
        const confirmed = await confirm({
          title: 'Confirmation required',
          description: formatOperationAssessment(assessment),
          confirmText: 'Create anyway',
          cancelText: 'Cancel',
        })
        if (!confirmed) {
          return
        }
        agent = await createAgent({
          ...request,
          confirmation_token: assessment.confirmation_token,
        })
      } else {
        throw error
      }
    }
    toast.success(t('workspace.agent.createSuccess'))
    const emittedModelRef = agent.agent.model_ref ?? {
      provider: selectedProvider as Provider,
      model: selectedModel as ModelId,
    }
    const emittedModel = emittedModelRef.model || models.value[0]?.model || 'gpt-5'
    emit('created', {
      id: agent.id,
      name: agent.name,
      model: emittedModel,
      model_ref: emittedModelRef,
    })
    emit('update:open', false)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.agent.createFailed')
    toast.error(message)
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
          <Label>{{ t('workspace.agent.providerLabel') }}</Label>
          <Select v-model="provider">
            <SelectTrigger>
              <SelectValue :placeholder="t('workspace.agent.providerPlaceholder')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="p in providers" :key="p" :value="p">
                {{ getProviderDisplayName(p) }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div class="space-y-2">
          <Label>
            {{ t('workspace.agent.modelLabel') }}
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
