<script setup lang="ts">
/**
 * CreateTaskDialog Component
 *
 * Dialog for creating new agent tasks with schedule configuration,
 * agent selection, and optional notification settings.
 */

import { ref, reactive, watch, computed } from 'vue'
import { Plus, Calendar, Bell, Terminal } from 'lucide-vue-next'
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
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TaskSchedule } from '@/types/generated/TaskSchedule'
import type { NotificationConfig } from '@/types/generated/NotificationConfig'
import type { ExecutionMode } from '@/types/generated/ExecutionMode'
import type { CreateAgentTaskRequest } from '@/api/agent-task'
import { listAgents } from '@/api/agents'
import { useToast } from '@/composables/useToast'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  create: [request: CreateAgentTaskRequest]
}>()

const toast = useToast()

// Form state
interface FormState {
  name: string
  description: string
  agentId: string
  input: string
  scheduleType: 'once' | 'interval' | 'cron'
  // Once schedule
  runAt: string // datetime-local format
  // Interval schedule
  intervalHours: number
  intervalMinutes: number
  // Cron schedule
  cronExpression: string
  cronTimezone: string
  // Execution mode
  executionModeType: 'api' | 'cli'
  cliBinary: string
  cliArgs: string
  cliWorkingDir: string
  cliTimeoutSecs: number
  cliUsePty: boolean
  // Notification
  notificationEnabled: boolean
  telegramBotToken: string
  telegramChatId: string
  notifyOnFailureOnly: boolean
  includeOutput: boolean
}

const initialState: FormState = {
  name: '',
  description: '',
  agentId: '',
  input: '',
  scheduleType: 'interval',
  runAt: '',
  intervalHours: 1,
  intervalMinutes: 0,
  cronExpression: '0 9 * * *',
  cronTimezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
  executionModeType: 'api',
  cliBinary: 'claude',
  cliArgs: '',
  cliWorkingDir: '',
  cliTimeoutSecs: 300,
  cliUsePty: false,
  notificationEnabled: false,
  telegramBotToken: '',
  telegramChatId: '',
  notifyOnFailureOnly: false,
  includeOutput: true,
}

const form = reactive<FormState>({ ...initialState })
const agents = ref<StoredAgent[]>([])
const isLoadingAgents = ref(false)
const isSubmitting = ref(false)

// Validation
const isValid = computed(() => {
  if (!form.name.trim()) return false
  if (!form.agentId) return false

  // Validate schedule
  if (form.scheduleType === 'once' && !form.runAt) return false
  if (form.scheduleType === 'interval' && form.intervalHours === 0 && form.intervalMinutes === 0)
    return false
  if (form.scheduleType === 'cron' && !form.cronExpression.trim()) return false

  // Validate execution mode
  if (form.executionModeType === 'cli' && !form.cliBinary.trim()) return false

  // Validate notification if enabled
  if (form.notificationEnabled) {
    if (!form.telegramBotToken.trim() || !form.telegramChatId.trim()) return false
  }

  return true
})

// Load agents when dialog opens
watch(
  () => props.open,
  async (isOpen) => {
    if (isOpen) {
      // Reset form
      Object.assign(form, initialState)

      // Load agents
      isLoadingAgents.value = true
      try {
        agents.value = await listAgents()
      } catch (error) {
        console.error('Failed to load agents:', error)
        toast.error('Failed to load agents')
      } finally {
        isLoadingAgents.value = false
      }
    }
  },
)

/**
 * Build schedule object from form state
 */
function buildSchedule(): TaskSchedule {
  switch (form.scheduleType) {
    case 'once':
      return {
        type: 'once',
        run_at: new Date(form.runAt).getTime(),
      }
    case 'interval':
      return {
        type: 'interval',
        interval_ms: (form.intervalHours * 60 + form.intervalMinutes) * 60 * 1000,
        start_at: null,
      }
    case 'cron':
      return {
        type: 'cron',
        expression: form.cronExpression,
        timezone: form.cronTimezone || null,
      }
  }
}

/**
 * Build notification config from form state
 */
function buildNotificationConfig(): NotificationConfig {
  return {
    telegram_enabled: form.notificationEnabled,
    telegram_bot_token: form.notificationEnabled ? form.telegramBotToken : null,
    telegram_chat_id: form.notificationEnabled ? form.telegramChatId : null,
    notify_on_failure_only: form.notifyOnFailureOnly,
    include_output: form.includeOutput,
  }
}

/**
 * Build execution mode config from form state
 */
function buildExecutionMode(): ExecutionMode {
  if (form.executionModeType === 'api') {
    return { type: 'api' }
  }
  // Parse args string into array (split by spaces, respecting quotes would be nice but keep it simple)
  const args = form.cliArgs.trim() ? form.cliArgs.trim().split(/\s+/) : []
  return {
    type: 'cli',
    binary: form.cliBinary,
    args,
    working_dir: form.cliWorkingDir.trim() || null,
    timeout_secs: BigInt(form.cliTimeoutSecs),
    use_pty: form.cliUsePty,
  }
}

/**
 * Handle form submission
 */
async function handleSubmit() {
  if (!isValid.value) return

  isSubmitting.value = true
  try {
    const request: CreateAgentTaskRequest = {
      name: form.name.trim(),
      agent_id: form.agentId,
      schedule: buildSchedule(),
      description: form.description.trim() || undefined,
      input: form.input.trim() || undefined,
      notification: buildNotificationConfig(),
      execution_mode: buildExecutionMode(),
    }

    emit('create', request)
    emit('update:open', false)
  } finally {
    isSubmitting.value = false
  }
}

function handleCancel() {
  emit('update:open', false)
}
</script>

<template>
  <Dialog :open="props.open" @update:open="emit('update:open', $event)">
    <DialogContent class="max-w-[600px] max-h-[80vh] flex flex-col">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <Plus :size="18" />
          Create Agent Task
        </DialogTitle>
        <DialogDescription> Schedule an agent to run automatically </DialogDescription>
      </DialogHeader>

      <form class="flex-1 overflow-y-auto space-y-4 py-4" @submit.prevent="handleSubmit">
        <!-- Basic Info -->
        <div class="space-y-3">
          <div class="space-y-1">
            <Label for="name">Name *</Label>
            <Input id="name" v-model="form.name" placeholder="My scheduled task" />
          </div>

          <div class="space-y-1">
            <Label for="description">Description</Label>
            <Textarea
              id="description"
              v-model="form.description"
              placeholder="What this task does..."
              :rows="2"
            />
          </div>
        </div>

        <!-- Agent Selection -->
        <div class="space-y-1">
          <Label for="agent">Agent *</Label>
          <Select v-model="form.agentId">
            <SelectTrigger id="agent" :disabled="isLoadingAgents">
              <SelectValue :placeholder="isLoadingAgents ? 'Loading...' : 'Select an agent'" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="agent in agents" :key="agent.id" :value="agent.id">
                {{ agent.name }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- Input/Prompt -->
        <div class="space-y-1">
          <Label for="input">Input / Prompt</Label>
          <Textarea
            id="input"
            v-model="form.input"
            placeholder="Optional input to send to the agent..."
            :rows="2"
          />
        </div>

        <!-- Execution Mode Section -->
        <div class="space-y-3 p-3 border rounded-lg bg-muted/30">
          <div class="flex items-center gap-2 text-sm font-medium">
            <Terminal :size="14" />
            Execution Mode
          </div>

          <div class="space-y-1">
            <Label for="executionMode">Mode</Label>
            <Select v-model="form.executionModeType">
              <SelectTrigger id="executionMode">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="api">API (RestFlow AI)</SelectItem>
                <SelectItem value="cli">CLI (External Tool)</SelectItem>
              </SelectContent>
            </Select>
            <p class="text-xs text-muted-foreground">
              {{ form.executionModeType === 'api' ? 'Use built-in RestFlow AI executor' : 'Use external CLI tool like Claude Code, Aider, etc.' }}
            </p>
          </div>

          <!-- CLI Configuration -->
          <template v-if="form.executionModeType === 'cli'">
            <div class="space-y-1">
              <Label for="cliBinary">CLI Binary *</Label>
              <Input
                id="cliBinary"
                v-model="form.cliBinary"
                placeholder="claude"
                class="font-mono"
              />
              <p class="text-xs text-muted-foreground">
                Command to run (e.g., "claude", "aider", "codex")
              </p>
            </div>

            <div class="space-y-1">
              <Label for="cliArgs">Arguments</Label>
              <Input
                id="cliArgs"
                v-model="form.cliArgs"
                placeholder="-p --dangerously-skip-permissions"
                class="font-mono"
              />
              <p class="text-xs text-muted-foreground">
                Additional CLI arguments (space-separated)
              </p>
            </div>

            <div class="space-y-1">
              <Label for="cliWorkingDir">Working Directory</Label>
              <Input
                id="cliWorkingDir"
                v-model="form.cliWorkingDir"
                placeholder="/path/to/project"
                class="font-mono"
              />
            </div>

            <div class="flex items-center gap-4">
              <div class="space-y-1 flex-1">
                <Label for="cliTimeout">Timeout (seconds)</Label>
                <Input
                  id="cliTimeout"
                  v-model.number="form.cliTimeoutSecs"
                  type="number"
                  min="30"
                  max="7200"
                  class="w-24"
                />
              </div>
              <label class="flex items-center gap-2 text-sm cursor-pointer pt-5">
                <input
                  v-model="form.cliUsePty"
                  type="checkbox"
                  class="h-4 w-4 rounded border-gray-300"
                />
                Use PTY (interactive mode)
              </label>
            </div>
          </template>
        </div>

        <!-- Schedule Section -->
        <div class="space-y-3 p-3 border rounded-lg bg-muted/30">
          <div class="flex items-center gap-2 text-sm font-medium">
            <Calendar :size="14" />
            Schedule
          </div>

          <div class="space-y-1">
            <Label for="scheduleType">Type</Label>
            <Select v-model="form.scheduleType">
              <SelectTrigger id="scheduleType">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="once">Once (run at specific time)</SelectItem>
                <SelectItem value="interval">Interval (recurring)</SelectItem>
                <SelectItem value="cron">Cron Expression</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Once Schedule -->
          <div v-if="form.scheduleType === 'once'" class="space-y-1">
            <Label for="runAt">Run At *</Label>
            <Input id="runAt" v-model="form.runAt" type="datetime-local" />
          </div>

          <!-- Interval Schedule -->
          <div v-if="form.scheduleType === 'interval'" class="space-y-1">
            <Label>Interval</Label>
            <div class="flex items-center gap-2">
              <Input
                v-model.number="form.intervalHours"
                type="number"
                min="0"
                max="168"
                class="w-20"
              />
              <span class="text-sm text-muted-foreground">hours</span>
              <Input
                v-model.number="form.intervalMinutes"
                type="number"
                min="0"
                max="59"
                class="w-20"
              />
              <span class="text-sm text-muted-foreground">minutes</span>
            </div>
          </div>

          <!-- Cron Schedule -->
          <div v-if="form.scheduleType === 'cron'" class="space-y-3">
            <div class="space-y-1">
              <Label for="cronExpression">Cron Expression *</Label>
              <Input
                id="cronExpression"
                v-model="form.cronExpression"
                placeholder="0 9 * * *"
                class="font-mono"
              />
              <p class="text-xs text-muted-foreground">
                Format: minute hour day month weekday (e.g., "0 9 * * *" for 9 AM daily)
              </p>
            </div>
            <div class="space-y-1">
              <Label for="cronTimezone">Timezone</Label>
              <Input
                id="cronTimezone"
                v-model="form.cronTimezone"
                placeholder="America/Los_Angeles"
              />
            </div>
          </div>
        </div>

        <!-- Notification Section -->
        <div class="space-y-3 p-3 border rounded-lg bg-muted/30">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2 text-sm font-medium">
              <Bell :size="14" />
              Telegram Notifications
            </div>
            <label class="flex items-center gap-2 text-sm cursor-pointer">
              <input
                v-model="form.notificationEnabled"
                type="checkbox"
                class="h-4 w-4 rounded border-gray-300"
              />
              Enable
            </label>
          </div>

          <template v-if="form.notificationEnabled">
            <div class="space-y-1">
              <Label for="telegramBotToken">Bot Token *</Label>
              <Input
                id="telegramBotToken"
                v-model="form.telegramBotToken"
                type="password"
                placeholder="123456:ABC-DEF..."
              />
            </div>

            <div class="space-y-1">
              <Label for="telegramChatId">Chat ID *</Label>
              <Input
                id="telegramChatId"
                v-model="form.telegramChatId"
                placeholder="-1001234567890"
              />
            </div>

            <div class="flex items-center gap-4">
              <label class="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  v-model="form.notifyOnFailureOnly"
                  type="checkbox"
                  class="h-4 w-4 rounded border-gray-300"
                />
                Only on failure
              </label>
              <label class="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  v-model="form.includeOutput"
                  type="checkbox"
                  class="h-4 w-4 rounded border-gray-300"
                />
                Include output
              </label>
            </div>
          </template>
        </div>
      </form>

      <DialogFooter>
        <Button variant="outline" @click="handleCancel" :disabled="isSubmitting"> Cancel </Button>
        <Button @click="handleSubmit" :disabled="!isValid || isSubmitting">
          <Plus :size="14" class="mr-1" />
          Create Task
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
