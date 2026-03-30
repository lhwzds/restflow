<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
import { Badge } from '@/components/ui/badge'
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
import { Switch } from '@/components/ui/switch'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  createHook,
  deleteHook,
  disableHook,
  enableHook,
  listHooks,
  testHook,
  updateHook,
  type CreateHookRequest,
  type UpdateHookRequest,
} from '@/api/hooks'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import type { Hook, HookAction, HookEvent, HookFilter } from '@/types/generated'

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()

const hooks = ref<Hook[]>([])
const loading = ref(false)
const saving = ref(false)
const showHookDialog = ref(false)
const editingHookId = ref<string | null>(null)
const actionInProgressId = ref<string | null>(null)
const error = ref<string | null>(null)

const form = reactive({
  name: '',
  description: '',
  event: 'task_completed' as HookEvent,
  actionType: 'webhook' as 'webhook' | 'script' | 'send_message' | 'run_task',
  webhookUrl: '',
  webhookMethod: 'POST',
  webhookHeaders: '',
  scriptPath: '',
  scriptArgs: '',
  channelType: 'telegram',
  messageTemplate: '',
  taskAgentId: '',
  taskInputTemplate: '',
  filterTaskPattern: '',
  filterAgentId: '',
  filterSuccessOnly: false,
})

const hookEvents: HookEvent[] = [
  'task_started',
  'task_completed',
  'task_failed',
  'task_interrupted',
]

function resetForm() {
  form.name = ''
  form.description = ''
  form.event = 'task_completed'
  form.actionType = 'webhook'
  form.webhookUrl = ''
  form.webhookMethod = 'POST'
  form.webhookHeaders = ''
  form.scriptPath = ''
  form.scriptArgs = ''
  form.channelType = 'telegram'
  form.messageTemplate = ''
  form.taskAgentId = ''
  form.taskInputTemplate = ''
  form.filterTaskPattern = ''
  form.filterAgentId = ''
  form.filterSuccessOnly = false
  editingHookId.value = null
}

function openCreateDialog() {
  resetForm()
  error.value = null
  showHookDialog.value = true
}

function openEditDialog(hook: Hook) {
  resetForm()
  error.value = null
  editingHookId.value = hook.id
  form.name = hook.name
  form.description = hook.description ?? ''
  form.event = hook.event
  form.filterTaskPattern = hook.filter?.task_name_pattern ?? ''
  form.filterAgentId = hook.filter?.agent_id ?? ''
  form.filterSuccessOnly = hook.filter?.success_only ?? false

  if (hook.action.type === 'webhook') {
    form.actionType = 'webhook'
    form.webhookUrl = hook.action.url
    form.webhookMethod = hook.action.method ?? 'POST'
    form.webhookHeaders = formatHeaders(hook.action.headers)
  } else if (hook.action.type === 'script') {
    form.actionType = 'script'
    form.scriptPath = hook.action.path
    form.scriptArgs = formatArgs(hook.action.args)
  } else if (hook.action.type === 'send_message') {
    form.actionType = 'send_message'
    form.channelType = hook.action.channel_type
    form.messageTemplate = hook.action.message_template
  } else {
    form.actionType = 'run_task'
    form.taskAgentId = hook.action.agent_id
    form.taskInputTemplate = hook.action.input_template
  }

  showHookDialog.value = true
}

function formatHeaders(headers: Record<string, string> | null): string {
  if (!headers) return ''
  return Object.entries(headers)
    .map(([key, value]) => `${key}: ${value}`)
    .join('\n')
}

function parseHeaders(raw: string): Record<string, string> | null {
  const value = raw.trim()
  if (!value) return null

  const headers: Record<string, string> = {}
  for (const line of value.split('\n')) {
    const [key, ...rest] = line.split(':')
    if (!key || rest.length === 0) continue
    headers[key.trim()] = rest.join(':').trim()
  }
  return Object.keys(headers).length > 0 ? headers : null
}

function formatArgs(args: string[] | null): string {
  if (!args || args.length === 0) return ''
  return args.join(' ')
}

function parseArgs(raw: string): string[] | null {
  const value = raw.trim()
  if (!value) return null
  const parts = value
    .split(/\s+/)
    .map((part) => part.trim())
    .filter(Boolean)
  return parts.length > 0 ? parts : null
}

function buildAction(): HookAction {
  if (form.actionType === 'webhook') {
    return {
      type: 'webhook',
      url: form.webhookUrl.trim(),
      method: form.webhookMethod.trim() || null,
      headers: parseHeaders(form.webhookHeaders),
    }
  }

  if (form.actionType === 'script') {
    return {
      type: 'script',
      path: form.scriptPath.trim(),
      args: parseArgs(form.scriptArgs),
      timeout_secs: null,
    }
  }

  if (form.actionType === 'send_message') {
    return {
      type: 'send_message',
      channel_type: form.channelType.trim() || 'telegram',
      message_template: form.messageTemplate.trim(),
    }
  }

  return {
    type: 'run_task',
    agent_id: form.taskAgentId.trim(),
    input_template: form.taskInputTemplate.trim(),
  }
}

function buildFilter(): HookFilter | null {
  const taskNamePattern = form.filterTaskPattern.trim()
  const agentId = form.filterAgentId.trim()
  const successOnly = form.filterSuccessOnly ? true : null

  if (!taskNamePattern && !agentId && !successOnly) {
    return null
  }

  return {
    task_name_pattern: taskNamePattern || null,
    agent_id: agentId || null,
    success_only: successOnly,
  }
}

function validateForm(): string | null {
  if (!form.name.trim()) {
    return t('settings.hooks.validationNameRequired')
  }

  if (form.actionType === 'webhook' && !form.webhookUrl.trim()) {
    return t('settings.hooks.validationWebhookRequired')
  }

  if (form.actionType === 'script' && !form.scriptPath.trim()) {
    return t('settings.hooks.validationScriptPathRequired')
  }

  if (form.actionType === 'send_message' && !form.messageTemplate.trim()) {
    return t('settings.hooks.validationMessageRequired')
  }

  if (form.actionType === 'run_task' && (!form.taskAgentId.trim() || !form.taskInputTemplate.trim())) {
    return t('settings.hooks.validationTaskRequired')
  }

  return null
}

async function loadAllHooks() {
  loading.value = true
  error.value = null
  try {
    hooks.value = await listHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function handleSaveHook() {
  const validationError = validateForm()
  if (validationError) {
    error.value = validationError
    return
  }

  const action = buildAction()
  const filter = buildFilter()
  saving.value = true
  error.value = null

  try {
    if (editingHookId.value) {
      const request: UpdateHookRequest = {
        name: form.name.trim(),
        description: form.description.trim() || null,
        event: form.event,
        action,
        filter,
      }
      await updateHook(editingHookId.value, request)
      toast.success(t('settings.hooks.updateSuccess'))
    } else {
      const request: CreateHookRequest = {
        name: form.name.trim(),
        description: form.description.trim() || null,
        event: form.event,
        action,
        filter,
        enabled: true,
      }
      await createHook(request)
      toast.success(t('settings.hooks.createSuccess'))
    }

    showHookDialog.value = false
    resetForm()
    await loadAllHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}

async function handleToggle(hook: Hook) {
  error.value = null
  actionInProgressId.value = hook.id
  try {
    if (hook.enabled) {
      await disableHook(hook.id)
      toast.success(t('settings.hooks.disabledSuccess'))
    } else {
      await enableHook(hook.id)
      toast.success(t('settings.hooks.enabledSuccess'))
    }
    await loadAllHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    actionInProgressId.value = null
  }
}

async function handleDelete(hook: Hook) {
  const confirmed = await confirm({
    title: t('settings.hooks.deleteConfirmTitle'),
    description: t('settings.hooks.deleteConfirmDescription', { name: hook.name }),
    confirmText: t('settings.hooks.delete'),
    cancelText: t('settings.hooks.cancel'),
    variant: 'destructive',
  })

  if (!confirmed) return

  error.value = null
  actionInProgressId.value = hook.id
  try {
    await deleteHook(hook.id)
    toast.success(t('settings.hooks.deleteSuccess'))
    await loadAllHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    actionInProgressId.value = null
  }
}

async function handleTest(hook: Hook) {
  error.value = null
  actionInProgressId.value = hook.id
  try {
    await testHook(hook.id)
    toast.success(t('settings.hooks.testSuccess'))
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    actionInProgressId.value = null
  }
}

function describeEvent(event: HookEvent): string {
  return t(`settings.hooks.events.${event}`)
}

function describeAction(action: HookAction): string {
  if (action.type === 'webhook') return t('settings.hooks.actionSummaryWebhook', { value: action.url })
  if (action.type === 'script') return t('settings.hooks.actionSummaryScript', { value: action.path })
  if (action.type === 'send_message') {
    return t('settings.hooks.actionSummaryMessage', { value: action.channel_type })
  }
  return t('settings.hooks.actionSummaryRunTask', { value: action.agent_id })
}

onMounted(() => {
  loadAllHooks()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">{{ t('settings.hooks.title') }}</h2>
        <p class="text-muted-foreground">{{ t('settings.hooks.description') }}</p>
      </div>
      <Button @click="openCreateDialog">{{ t('settings.hooks.addHook') }}</Button>
    </div>

    <Dialog v-model:open="showHookDialog">
        <DialogContent class="max-w-[42rem]">
          <DialogHeader>
            <DialogTitle>
              {{ editingHookId ? t('settings.hooks.editHook') : t('settings.hooks.createHook') }}
            </DialogTitle>
            <DialogDescription>
              {{
                editingHookId
                  ? t('settings.hooks.editHookDescription')
                  : t('settings.hooks.createHookDescription')
              }}
            </DialogDescription>
          </DialogHeader>

          <div class="grid gap-4 py-2">
            <div class="grid gap-2">
              <Label for="hook-name">{{ t('settings.hooks.nameLabel') }}</Label>
              <Input id="hook-name" v-model="form.name" :placeholder="t('settings.hooks.namePlaceholder')" />
            </div>

            <div class="grid gap-2">
              <Label for="hook-description">{{ t('settings.hooks.descriptionLabel') }}</Label>
              <Input
                id="hook-description"
                v-model="form.description"
                :placeholder="t('settings.hooks.descriptionPlaceholder')"
              />
            </div>

            <div class="grid grid-cols-2 gap-3">
              <div class="grid gap-2">
                <Label>{{ t('settings.hooks.eventLabel') }}</Label>
                <Select v-model="form.event">
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem
                      v-for="event in hookEvents"
                      :key="event"
                      :value="event"
                    >
                      {{ describeEvent(event) }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div class="grid gap-2">
                <Label>{{ t('settings.hooks.actionTypeLabel') }}</Label>
                <Select v-model="form.actionType">
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="webhook">{{ t('settings.hooks.actionWebhook') }}</SelectItem>
                    <SelectItem value="script">{{ t('settings.hooks.actionScript') }}</SelectItem>
                    <SelectItem value="send_message">{{ t('settings.hooks.actionMessage') }}</SelectItem>
                    <SelectItem value="run_task">{{ t('settings.hooks.actionRunTask') }}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <template v-if="form.actionType === 'webhook'">
              <div class="grid gap-2">
                <Label for="webhook-url">{{ t('settings.hooks.webhookUrlLabel') }}</Label>
                <Input
                  id="webhook-url"
                  v-model="form.webhookUrl"
                  :placeholder="t('settings.hooks.webhookUrlPlaceholder')"
                />
              </div>
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="webhook-method">{{ t('settings.hooks.webhookMethodLabel') }}</Label>
                  <Input
                    id="webhook-method"
                    v-model="form.webhookMethod"
                    :placeholder="t('settings.hooks.webhookMethodPlaceholder')"
                  />
                </div>
                <div class="grid gap-2">
                  <Label for="webhook-headers">{{ t('settings.hooks.webhookHeadersLabel') }}</Label>
                  <Input
                    id="webhook-headers"
                    v-model="form.webhookHeaders"
                    :placeholder="t('settings.hooks.webhookHeadersPlaceholder')"
                  />
                </div>
              </div>
            </template>

            <template v-else-if="form.actionType === 'script'">
              <div class="grid gap-2">
                <Label for="script-path">{{ t('settings.hooks.scriptPathLabel') }}</Label>
                <Input
                  id="script-path"
                  v-model="form.scriptPath"
                  :placeholder="t('settings.hooks.scriptPathPlaceholder')"
                />
              </div>
              <div class="grid gap-2">
                <Label for="script-args">{{ t('settings.hooks.scriptArgsLabel') }}</Label>
                <Input
                  id="script-args"
                  v-model="form.scriptArgs"
                  :placeholder="t('settings.hooks.scriptArgsPlaceholder')"
                />
              </div>
            </template>

            <template v-else-if="form.actionType === 'send_message'">
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label>{{ t('settings.hooks.channelTypeLabel') }}</Label>
                  <Select v-model="form.channelType">
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="telegram">Telegram</SelectItem>
                      <SelectItem value="discord">Discord</SelectItem>
                      <SelectItem value="slack">Slack</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div class="grid gap-2">
                  <Label for="message-template">{{ t('settings.hooks.messageTemplateLabel') }}</Label>
                  <Input
                    id="message-template"
                    v-model="form.messageTemplate"
                    :placeholder="t('settings.hooks.messageTemplatePlaceholder')"
                  />
                </div>
              </div>
            </template>

            <template v-else>
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="task-agent-id">{{ t('settings.hooks.taskAgentIdLabel') }}</Label>
                  <Input
                    id="task-agent-id"
                    v-model="form.taskAgentId"
                    :placeholder="t('settings.hooks.taskAgentIdPlaceholder')"
                  />
                </div>
                <div class="grid gap-2">
                  <Label for="task-input-template">{{ t('settings.hooks.taskInputTemplateLabel') }}</Label>
                  <Input
                    id="task-input-template"
                    v-model="form.taskInputTemplate"
                    :placeholder="t('settings.hooks.taskInputTemplatePlaceholder')"
                  />
                </div>
              </div>
            </template>

            <div class="rounded-lg border bg-muted/30 p-3 space-y-3">
              <h4 class="text-sm font-medium">{{ t('settings.hooks.filterTitle') }}</h4>
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="filter-task-pattern">{{ t('settings.hooks.filterTaskPatternLabel') }}</Label>
                  <Input
                    id="filter-task-pattern"
                    v-model="form.filterTaskPattern"
                    :placeholder="t('settings.hooks.filterTaskPatternPlaceholder')"
                  />
                </div>
                <div class="grid gap-2">
                  <Label for="filter-agent-id">{{ t('settings.hooks.filterAgentIdLabel') }}</Label>
                  <Input
                    id="filter-agent-id"
                    v-model="form.filterAgentId"
                    :placeholder="t('settings.hooks.filterAgentIdPlaceholder')"
                  />
                </div>
              </div>
              <div class="flex items-center justify-between rounded-md border bg-background px-3 py-2">
                <Label for="filter-success-only">{{ t('settings.hooks.filterSuccessOnlyLabel') }}</Label>
                <Switch
                  id="filter-success-only"
                  :checked="form.filterSuccessOnly"
                  @update:checked="(value) => (form.filterSuccessOnly = Boolean(value))"
                />
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" @click="showHookDialog = false">{{ t('settings.hooks.cancel') }}</Button>
            <Button :disabled="saving" @click="handleSaveHook">
              {{ editingHookId ? t('settings.hooks.save') : t('settings.hooks.create') }}
            </Button>
          </DialogFooter>
        </DialogContent>
    </Dialog>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div v-if="loading" class="flex items-center gap-2 text-sm text-muted-foreground">
      <Loader2 class="h-4 w-4 animate-spin" />
      {{ t('settings.hooks.loading') }}
    </div>

    <div v-else-if="hooks.length === 0" class="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
      {{ t('settings.hooks.empty') }}
    </div>

    <div v-else class="space-y-3">
      <div
        v-for="hook in hooks"
        :key="hook.id"
        class="rounded-lg border bg-card p-4"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="space-y-1">
            <div class="flex items-center gap-2">
              <h3 class="font-medium">{{ hook.name }}</h3>
              <Badge variant="outline">{{ describeEvent(hook.event) }}</Badge>
              <Badge :variant="hook.enabled ? 'default' : 'secondary'">
                {{ hook.enabled ? t('settings.hooks.enabled') : t('settings.hooks.disabled') }}
              </Badge>
            </div>
            <p v-if="hook.description" class="text-sm text-muted-foreground">{{ hook.description }}</p>
            <p class="text-xs text-muted-foreground">{{ describeAction(hook.action) }}</p>
          </div>

          <div class="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              :disabled="actionInProgressId === hook.id"
              @click="openEditDialog(hook)"
            >
              {{ t('settings.hooks.edit') }}
            </Button>
            <Button
              variant="outline"
              size="sm"
              :disabled="actionInProgressId === hook.id"
              @click="handleTest(hook)"
            >
              {{ t('settings.hooks.test') }}
            </Button>
            <Button
              variant="outline"
              size="sm"
              :disabled="actionInProgressId === hook.id"
              @click="handleToggle(hook)"
            >
              {{ hook.enabled ? t('settings.hooks.disable') : t('settings.hooks.enable') }}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              :disabled="actionInProgressId === hook.id"
              @click="handleDelete(hook)"
            >
              {{ t('settings.hooks.delete') }}
            </Button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
