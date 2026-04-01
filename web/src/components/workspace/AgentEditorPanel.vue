<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowLeft } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { getAgent, updateAgent } from '@/api/agents'
import { listSkills } from '@/api/skills'
import { getAvailableTools } from '@/api/config'
import { useModelsStore } from '@/stores/modelsStore'
import { useToast } from '@/composables/useToast'
import type { ModelId } from '@/types/generated/ModelId'
import type { Provider } from '@/types/generated/Provider'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { WorkspaceAgentModelSelection } from '@/types/workspace'
import { getProviderDisplayName } from '@/utils/providerCatalog'

const props = defineProps<{
  agentId: string | null
}>()

const emit = defineEmits<{
  backToSessions: []
  updated: [agent: WorkspaceAgentModelSelection]
}>()

const { t } = useI18n()
const toast = useToast()
const modelsStore = useModelsStore()

const loading = ref(false)
const saving = ref(false)
const current = ref<StoredAgent | null>(null)
const name = ref('')
const provider = ref<Provider | ''>('')
const model = ref<ModelId | ''>('')
const temperature = ref('')
const prompt = ref('')
const availableToolCount = ref(0)
const availableSkillCount = ref(0)

const providers = computed(() => modelsStore.getProviders)
const models = computed(() =>
  provider.value ? modelsStore.getModelsByProvider(provider.value as Provider) : [],
)
const hasAgent = computed(() => !!props.agentId)
const effectiveToolCount = computed(() => {
  const configured = current.value?.agent.tools
  if (configured && configured.length > 0) return configured.length
  return availableToolCount.value
})
const effectiveSkillCount = computed(() => {
  const configured = current.value?.agent.skills
  if (configured && configured.length > 0) return configured.length
  return availableSkillCount.value
})
const templateType = computed(() => {
  const promptFile = current.value?.prompt_file?.toLowerCase()
  if (promptFile === 'default.md') return t('workspace.agent.templateDefault')
  if (promptFile === 'background_agent.md') return t('workspace.agent.templateBackground')
  return t('workspace.agent.templateCustom')
})

function applyForm(agent: StoredAgent) {
  current.value = agent
  name.value = agent.name
  const resolvedModel = agent.agent.model ?? ''
  model.value = resolvedModel
  const inferredProvider =
    agent.agent.model_ref?.provider ??
    (resolvedModel ? (modelsStore.getModelMetadata(resolvedModel as ModelId)?.provider ?? '') : '')
  provider.value = inferredProvider
  temperature.value =
    typeof agent.agent.temperature === 'number' ? String(agent.agent.temperature) : ''
  prompt.value = agent.agent.prompt ?? ''
}

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

async function loadAgent(agentId: string) {
  loading.value = true
  try {
    const agent = await getAgent(agentId)
    applyForm(agent)
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.agent.loadDetailsFailed')
    toast.error(message)
  } finally {
    loading.value = false
  }
}

watch(
  () => props.agentId,
  (agentId) => {
    if (!agentId) {
      current.value = null
      return
    }
    void loadAgent(agentId)
  },
  { immediate: true },
)

onMounted(() => {
  void modelsStore.loadModels().catch(() => {
    toast.error(t('chat.loadModelsFailed'))
  })
  void loadReferenceCounts()
})

async function loadReferenceCounts() {
  try {
    const [tools, skills] = await Promise.all([
      getAvailableTools(),
      listSkills(),
    ])
    availableToolCount.value = tools.length
    availableSkillCount.value = skills.length
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.agent.loadDetailsFailed')
    toast.error(message)
  }
}

async function save() {
  if (!props.agentId || !current.value) return
  const agentId = props.agentId
  const nextName = name.value.trim()
  if (!nextName) return
  if (!provider.value || !model.value) {
    toast.error(t('workspace.agent.providerModelRequired'))
    return
  }

  const parsedTemperature =
    temperature.value.trim() === '' ? undefined : Number(temperature.value.trim())
  if (parsedTemperature !== undefined && Number.isNaN(parsedTemperature)) {
    toast.error(t('workspace.agent.invalidTemperature'))
    return
  }

  saving.value = true
  try {
    const request = {
      name: nextName,
      agent: {
        ...current.value.agent,
        model: model.value ? (model.value as ModelId) : undefined,
        model_ref:
          provider.value && model.value
            ? {
                provider: provider.value as Provider,
                model: model.value as ModelId,
              }
            : undefined,
        prompt: prompt.value.trim() || undefined,
        temperature: parsedTemperature,
      },
    }
    const updated = await updateAgent(agentId, request)
    applyForm(updated)
    const emittedModelRef =
      updated.agent.model_ref ??
      (provider.value && model.value
        ? {
            provider: provider.value as Provider,
            model: model.value as ModelId,
          }
        : undefined)
    if (!emittedModelRef) {
      throw new Error('Missing model_ref after agent update')
    }
    emit('updated', {
      id: updated.id,
      name: updated.name,
      model: emittedModelRef.model || 'gpt-5',
      model_ref: emittedModelRef,
    })
    toast.success(t('workspace.agent.saveSuccess'))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.agent.saveFailed')
    toast.error(message)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
    <div class="h-8 shrink-0 border-b border-border px-2 flex items-center">
      <Button variant="ghost" size="sm" class="h-7 gap-1.5" @click="emit('backToSessions')">
        <ArrowLeft :size="14" />
        {{ t('workspace.agent.backToSessions') }}
      </Button>
    </div>

    <div class="flex-1 overflow-auto p-4">
      <div
        v-if="!hasAgent"
        class="h-full flex items-center justify-center text-sm text-muted-foreground"
      >
        {{ t('workspace.agent.selectHint') }}
      </div>

      <div
        v-else-if="loading"
        class="h-full flex items-center justify-center text-sm text-muted-foreground"
      >
        {{ t('workspace.agent.loading') }}
      </div>

      <div v-else-if="current" class="mx-auto w-full max-w-[42rem] space-y-5">
        <div class="space-y-1">
          <h2 class="text-lg font-semibold">{{ t('workspace.agent.detailsTitle') }}</h2>
          <div class="text-xs text-muted-foreground font-mono">{{ current.id }}</div>
        </div>

        <div class="space-y-2">
          <Label>{{ t('workspace.agent.nameLabel') }}</Label>
          <Input v-model="name" :placeholder="t('workspace.agent.namePlaceholder')" />
        </div>

        <div class="space-y-2">
          <Label>{{ t('workspace.agent.providerLabel') }}</Label>
          <Select
            :model-value="provider"
            @update:model-value="provider = $event as Provider | ''"
          >
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
          <Label>{{ t('workspace.agent.modelLabel') }}</Label>
          <Select :model-value="model" @update:model-value="model = $event as ModelId | ''">
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

        <div class="space-y-2">
          <Label>{{ t('workspace.agent.temperature') }}</Label>
          <Input v-model="temperature" placeholder="0.7" />
        </div>

        <div class="space-y-2">
          <Label>{{ t('workspace.agent.prompt') }}</Label>
          <Textarea
            v-model="prompt"
            class="min-h-[12rem]"
            :placeholder="t('workspace.agent.promptPlaceholder')"
          />
        </div>

        <div class="grid grid-cols-3 gap-3 text-sm">
          <div class="rounded-md border border-border px-3 py-2">
            <div class="text-xs text-muted-foreground">{{ t('workspace.agent.tools') }}</div>
            <div data-testid="agent-tool-count" class="font-medium">{{ effectiveToolCount }}</div>
          </div>
          <div class="rounded-md border border-border px-3 py-2">
            <div class="text-xs text-muted-foreground">{{ t('workspace.agent.skills') }}</div>
            <div data-testid="agent-skill-count" class="font-medium">{{ effectiveSkillCount }}</div>
          </div>
          <div class="rounded-md border border-border px-3 py-2">
            <div class="text-xs text-muted-foreground">{{ t('workspace.agent.templateType') }}</div>
            <div data-testid="agent-template-type" class="font-medium">{{ templateType }}</div>
          </div>
        </div>

        <div class="flex justify-end">
          <Button :disabled="saving || !name.trim()" @click="save">
            {{ t('workspace.agent.save') }}
          </Button>
        </div>
      </div>
    </div>
  </div>
</template>
