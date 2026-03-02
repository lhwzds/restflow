<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import {
  createHook,
  deleteHook,
  disableHook,
  enableHook,
  listHooks,
  testHook,
  type CreateHookRequest,
} from '@/api/hooks'
import type { Hook, HookAction, HookEvent } from '@/types/generated'

const hooks = ref<Hook[]>([])
const loading = ref(false)
const saving = ref(false)
const showCreateDialog = ref(false)
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
})

const hookEvents: HookEvent[] = [
  'task_started',
  'task_completed',
  'task_failed',
  'task_cancelled',
  'tool_executed',
  'approval_required',
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

async function handleCreateHook() {
  if (!form.name.trim()) {
    error.value = 'Hook name is required.'
    return
  }

  if (form.actionType === 'webhook' && !form.webhookUrl.trim()) {
    error.value = 'Webhook URL is required.'
    return
  }

  if (form.actionType === 'script' && !form.scriptPath.trim()) {
    error.value = 'Script path is required.'
    return
  }

  if (form.actionType === 'send_message' && !form.messageTemplate.trim()) {
    error.value = 'Message template is required.'
    return
  }

  if (form.actionType === 'run_task' && (!form.taskAgentId.trim() || !form.taskInputTemplate.trim())) {
    error.value = 'Agent ID and input template are required.'
    return
  }

  const request: CreateHookRequest = {
    name: form.name.trim(),
    description: form.description.trim() || null,
    event: form.event,
    action: buildAction(),
    enabled: true,
  }

  saving.value = true
  error.value = null
  try {
    await createHook(request)
    showCreateDialog.value = false
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
  try {
    if (hook.enabled) {
      await disableHook(hook.id)
    } else {
      await enableHook(hook.id)
    }
    await loadAllHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function handleDelete(hook: Hook) {
  if (!window.confirm(`Delete hook "${hook.name}"?`)) {
    return
  }
  error.value = null
  try {
    await deleteHook(hook.id)
    await loadAllHooks()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function handleTest(hook: Hook) {
  error.value = null
  try {
    await testHook(hook.id)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

function describeAction(action: HookAction): string {
  if (action.type === 'webhook') return `Webhook: ${action.url}`
  if (action.type === 'script') return `Script: ${action.path}`
  if (action.type === 'send_message') return `Message: ${action.channel_type}`
  return `Run task: ${action.agent_id}`
}

onMounted(() => {
  loadAllHooks()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">Hooks</h2>
        <p class="text-muted-foreground">Manage lifecycle hooks exposed from the backend.</p>
      </div>

      <Dialog v-model:open="showCreateDialog">
        <DialogTrigger as-child>
          <Button>Add Hook</Button>
        </DialogTrigger>
        <DialogContent class="max-w-[42rem]">
          <DialogHeader>
            <DialogTitle>Create Hook</DialogTitle>
            <DialogDescription>Configure event and action for the new hook.</DialogDescription>
          </DialogHeader>

          <div class="grid gap-4 py-2">
            <div class="grid gap-2">
              <Label for="hook-name">Name</Label>
              <Input id="hook-name" v-model="form.name" placeholder="Task failure notifier" />
            </div>

            <div class="grid gap-2">
              <Label for="hook-description">Description</Label>
              <Input id="hook-description" v-model="form.description" placeholder="Optional description" />
            </div>

            <div class="grid grid-cols-2 gap-3">
              <div class="grid gap-2">
                <Label>Event</Label>
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
                      {{ event }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div class="grid gap-2">
                <Label>Action Type</Label>
                <Select v-model="form.actionType">
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="webhook">webhook</SelectItem>
                    <SelectItem value="script">script</SelectItem>
                    <SelectItem value="send_message">send_message</SelectItem>
                    <SelectItem value="run_task">run_task</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <template v-if="form.actionType === 'webhook'">
              <div class="grid gap-2">
                <Label for="webhook-url">Webhook URL</Label>
                <Input id="webhook-url" v-model="form.webhookUrl" placeholder="https://example.com/hook" />
              </div>
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="webhook-method">Method</Label>
                  <Input id="webhook-method" v-model="form.webhookMethod" placeholder="POST" />
                </div>
                <div class="grid gap-2">
                  <Label for="webhook-headers">Headers (one per line)</Label>
                  <Input id="webhook-headers" v-model="form.webhookHeaders" placeholder="Authorization: Bearer token" />
                </div>
              </div>
            </template>

            <template v-else-if="form.actionType === 'script'">
              <div class="grid gap-2">
                <Label for="script-path">Script Path</Label>
                <Input id="script-path" v-model="form.scriptPath" placeholder="/path/to/script.sh" />
              </div>
              <div class="grid gap-2">
                <Label for="script-args">Args (space-separated)</Label>
                <Input id="script-args" v-model="form.scriptArgs" placeholder="--flag value" />
              </div>
            </template>

            <template v-else-if="form.actionType === 'send_message'">
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="channel-type">Channel Type</Label>
                  <Input id="channel-type" v-model="form.channelType" placeholder="telegram" />
                </div>
                <div class="grid gap-2">
                  <Label for="message-template">Message Template</Label>
                  <Input id="message-template" v-model="form.messageTemplate" placeholder="Task {{task_name}} failed" />
                </div>
              </div>
            </template>

            <template v-else>
              <div class="grid grid-cols-2 gap-3">
                <div class="grid gap-2">
                  <Label for="task-agent-id">Agent ID</Label>
                  <Input id="task-agent-id" v-model="form.taskAgentId" placeholder="default" />
                </div>
                <div class="grid gap-2">
                  <Label for="task-input-template">Input Template</Label>
                  <Input id="task-input-template" v-model="form.taskInputTemplate" placeholder="Investigate: {{error}}" />
                </div>
              </div>
            </template>
          </div>

          <DialogFooter>
            <Button variant="outline" @click="showCreateDialog = false">Cancel</Button>
            <Button :disabled="saving" @click="handleCreateHook">Create</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div v-if="loading" class="text-sm text-muted-foreground">Loading hooks...</div>

    <div v-else-if="hooks.length === 0" class="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
      No hooks found.
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
              <Badge variant="outline">{{ hook.event }}</Badge>
              <Badge :variant="hook.enabled ? 'default' : 'secondary'">
                {{ hook.enabled ? 'enabled' : 'disabled' }}
              </Badge>
            </div>
            <p v-if="hook.description" class="text-sm text-muted-foreground">{{ hook.description }}</p>
            <p class="text-xs text-muted-foreground">{{ describeAction(hook.action) }}</p>
          </div>

          <div class="flex items-center gap-2">
            <Button variant="outline" size="sm" @click="handleTest(hook)">Test</Button>
            <Button variant="outline" size="sm" @click="handleToggle(hook)">
              {{ hook.enabled ? 'Disable' : 'Enable' }}
            </Button>
            <Button variant="destructive" size="sm" @click="handleDelete(hook)">Delete</Button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
