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
import { useModelsStore } from '@/stores/modelsStore'
import { useToast } from '@/composables/useToast'
import type { AIModel } from '@/types/generated/AIModel'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const props = defineProps<{
  agentId: string | null
}>()

const emit = defineEmits<{
  backToSessions: []
  updated: [agent: { id: string; name: string; model: string }]
}>()

const { t } = useI18n()
const toast = useToast()
const modelsStore = useModelsStore()

const loading = ref(false)
const saving = ref(false)
const current = ref<StoredAgent | null>(null)
const name = ref('')
const model = ref('')
const temperature = ref('')
const prompt = ref('')

const models = computed(() => modelsStore.getAllModels)
const hasAgent = computed(() => !!props.agentId)

function applyForm(agent: StoredAgent) {
  current.value = agent
  name.value = agent.name
  model.value = agent.agent.model ?? ''
  temperature.value =
    typeof agent.agent.temperature === 'number' ? String(agent.agent.temperature) : ''
  prompt.value = agent.agent.prompt ?? ''
}

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
})

async function save() {
  if (!props.agentId || !current.value) return
  const nextName = name.value.trim()
  if (!nextName) return

  const parsedTemperature =
    temperature.value.trim() === '' ? undefined : Number(temperature.value.trim())
  if (parsedTemperature !== undefined && Number.isNaN(parsedTemperature)) {
    toast.error(t('workspace.agent.invalidTemperature'))
    return
  }

  saving.value = true
  try {
    const updated = await updateAgent(props.agentId, {
      name: nextName,
      agent: {
        ...current.value.agent,
        model: model.value ? (model.value as AIModel) : undefined,
        prompt: prompt.value.trim() || undefined,
        temperature: parsedTemperature,
      },
    })
    applyForm(updated)
    emit('updated', {
      id: updated.id,
      name: updated.name,
      model: (updated.agent.model ?? model.value) || 'gpt-5',
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
    <div class="h-8 shrink-0 border-b border-border px-2 flex items-center" data-tauri-drag-region>
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
          <Label>{{ t('workspace.agent.modelLabel') }}</Label>
          <Select :model-value="model" @update:model-value="model = $event">
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

        <div class="grid grid-cols-2 gap-3 text-sm">
          <div class="rounded-md border border-border px-3 py-2">
            <div class="text-xs text-muted-foreground">{{ t('workspace.agent.tools') }}</div>
            <div class="font-medium">{{ current.agent.tools?.length ?? 0 }}</div>
          </div>
          <div class="rounded-md border border-border px-3 py-2">
            <div class="text-xs text-muted-foreground">{{ t('workspace.agent.skills') }}</div>
            <div class="font-medium">{{ current.agent.skills?.length ?? 0 }}</div>
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
