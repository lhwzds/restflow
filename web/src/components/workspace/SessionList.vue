<script setup lang="ts">
import {
  Plus,
  Check,
  Activity,
  Loader2,
  MoreHorizontal,
  Pencil,
  Trash2,
  Archive,
  ArrowLeftFromLine,
  ArrowRightFromLine,
  RotateCcw,
  ChevronDown,
  ChevronRight,
  Radio,
  MessageSquare,
} from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { TIME_THRESHOLDS, TIME_UNITS } from '@/constants'
import type {
  BackgroundTaskFolder,
  ExternalChannelFolder,
  RunListItem,
  WorkspaceSessionFolder,
} from '@/types/workspace'
import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'

const props = withDefaults(
  defineProps<{
    workspaceFolders: WorkspaceSessionFolder[]
    backgroundFolders?: BackgroundTaskFolder[]
    externalFolders?: ExternalChannelFolder[]
    currentContainerId?: string | null
    currentRunId?: string | null
  }>(),
  {
    backgroundFolders: () => [],
    externalFolders: () => [],
    currentContainerId: null,
    currentRunId: null,
  },
)

const { t } = useI18n()

const emit = defineEmits<{
  newSession: []
  selectContainer: [kind: 'workspace' | 'background_task' | 'external_channel', containerId: string]
  selectRun: [containerId: string, runId: string]
  rename: [id: string, currentName: string]
  archive: [id: string, name: string]
  delete: [id: string, name: string]
  convertToBackgroundAgent: [id: string, name: string]
  convertToWorkspaceSession: [id: string, name: string]
  rebuild: [id: string, name: string]
  toggleWorkspaceFolder: [containerId: string]
  toggleBackgroundTask: [taskId: string]
  toggleExternalChannel: [containerId: string]
}>()

const DISPLAY_PREFIXES = ['channel:', 'background:']

function displayLabel(name: string): string {
  const trimmedName = name.trimStart()
  const normalized = trimmedName.toLowerCase()

  for (const prefix of DISPLAY_PREFIXES) {
    if (!normalized.startsWith(prefix)) {
      continue
    }

    const displayName = trimmedName.slice(prefix.length).trim()
    return displayName || name
  }

  return name
}

function sourceLabel(source: ChatSessionSource | null | undefined): string | null {
  if (!source) return null

  switch (source) {
    case 'workspace':
      return t('workspace.sessionSource.workspace')
    case 'telegram':
      return t('workspace.sessionSource.telegram')
    case 'discord':
      return t('workspace.sessionSource.discord')
    case 'slack':
      return t('workspace.sessionSource.slack')
    case 'external_legacy':
      return t('workspace.sessionSource.externalLegacy')
    default:
      return null
  }
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < TIME_THRESHOLDS.SECONDS_AGO) return t('workspace.time.justNow')
  if (diff < TIME_THRESHOLDS.MINUTES_AGO)
    return t('workspace.time.minutesAgo', { count: Math.floor(diff / TIME_UNITS.MS_PER_MINUTE) })
  if (diff < TIME_THRESHOLDS.HOURS_AGO)
    return t('workspace.time.hoursAgo', { count: Math.floor(diff / TIME_UNITS.MS_PER_HOUR) })
  return date.toLocaleDateString()
}

function normalizeStatusIcon(status: string) {
  if (status === 'running') return Loader2
  if (status === 'completed') return Check
  return Activity
}

function runKey(containerId: string, run: RunListItem): string {
  return `${containerId}:${run.runId ?? run.id}`
}

function isRunSelected(runId: string | null | undefined): boolean {
  return !!runId && props.currentRunId === runId
}

function isContainerSelected(containerId: string): boolean {
  return props.currentContainerId === containerId && !props.currentRunId
}
</script>

<template>
  <div class="flex min-h-0 flex-col bg-muted/30">
    <div class="space-y-2 px-3 pb-3 pt-2">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('newSession')">
        <Plus :size="16" />
        <span>{{ t('workspace.newSession') }}</span>
      </Button>
    </div>

    <div class="flex-1 overflow-auto py-2">
      <div class="px-3 pb-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Workspace Sessions
      </div>
      <div
        v-for="folder in workspaceFolders"
        :key="folder.containerId"
        :data-testid="`workspace-folder-${folder.containerId}`"
      >
        <div
          :class="
            cn(
              'flex items-start gap-2 px-3 py-2 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.containerId) && 'bg-muted',
            )
          "
        >
          <button
            class="mt-0.5 shrink-0 text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse workspace folder' : 'Expand workspace folder'"
            @click="emit('toggleWorkspaceFolder', folder.containerId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="14" />
          </button>
          <MessageSquare :size="14" class="mt-0.5 shrink-0 text-muted-foreground" />
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'workspace', folder.containerId)"
          >
            <div class="truncate text-sm">{{ displayLabel(folder.name) }}</div>
            <div class="truncate text-xs text-muted-foreground">
              <span v-if="folder.agentName">{{ folder.agentName }}</span>
              <span v-else>{{ t('common.unknownAgent') }}</span>
            </div>
            <div class="text-xs text-muted-foreground">
              {{ folder.subtitle || formatTime(folder.updatedAt) }}
            </div>
          </button>
          <div class="shrink-0 self-start" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="14" class="text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" class="w-48">
                <DropdownMenuItem
                  @click="emit('rename', folder.sessionId, displayLabel(folder.name))"
                >
                  <Pencil :size="14" class="mr-2" />
                  {{ t('workspace.session.rename') }}
                </DropdownMenuItem>
                <DropdownMenuItem
                  @click="emit('convertToBackgroundAgent', folder.sessionId, displayLabel(folder.name))"
                >
                  <ArrowRightFromLine :size="14" class="mr-2" />
                  {{ t('workspace.session.convertToBackground') }}
                </DropdownMenuItem>
                <DropdownMenuItem
                  @click="emit('archive', folder.sessionId, displayLabel(folder.name))"
                >
                  <Archive :size="14" class="mr-2" />
                  {{ t('workspace.session.archive') }}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  class="text-destructive focus:text-destructive"
                  @click="emit('delete', folder.sessionId, displayLabel(folder.name))"
                >
                  <Trash2 :size="14" class="mr-2" />
                  {{ t('workspace.session.delete') }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <div v-if="folder.expanded" class="pb-1">
          <button
            v-for="run in folder.runs"
            :key="runKey(folder.containerId, run)"
            :data-testid="`workspace-run-${folder.containerId}-${run.runId ?? 'latest'}`"
            :class="
              cn(
                'flex w-full items-start gap-2 px-9 py-2 text-left transition-colors hover:bg-muted/50',
                isRunSelected(run.runId) && 'bg-muted',
              )
            "
            @click="run.runId && emit('selectRun', folder.containerId, run.runId)"
          >
            <component
              :is="normalizeStatusIcon(run.status)"
              :size="12"
              :class="
                cn(
                  'mt-0.5 shrink-0 text-muted-foreground',
                  run.status === 'running' && 'animate-spin text-primary',
                  run.status === 'completed' && 'text-green-500',
                )
              "
            />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm">{{ run.title }}</div>
              <div class="text-xs text-muted-foreground">
                {{ formatTime(run.updatedAt) }}
              </div>
            </div>
          </button>
          <button
            v-if="folder.runs.length === 0"
            class="w-full px-9 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/50"
            data-testid="workspace-run-empty"
            @click="emit('selectContainer', 'workspace', folder.containerId)"
          >
            No runs yet
          </button>
        </div>
      </div>

      <div
        class="px-3 pb-2 pt-4 text-[11px] font-medium uppercase tracking-wide text-muted-foreground"
      >
        Background Agents
      </div>
      <div
        v-for="folder in backgroundFolders"
        :key="folder.taskId"
        :data-testid="`background-folder-${folder.taskId}`"
      >
        <div
          :class="
            cn(
              'flex items-start gap-2 px-3 py-2 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.taskId) && 'bg-muted',
            )
          "
        >
          <button
            class="mt-0.5 shrink-0 text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse background folder' : 'Expand background folder'"
            @click="emit('toggleBackgroundTask', folder.taskId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="14" />
          </button>
          <component
            :is="normalizeStatusIcon(folder.status)"
            :size="14"
            :class="
              cn(
                'mt-0.5 shrink-0 text-muted-foreground',
                folder.status === 'running' && 'animate-spin text-primary',
                folder.status === 'completed' && 'text-green-500',
              )
            "
          />
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'background_task', folder.taskId)"
          >
            <div class="truncate text-sm">{{ folder.name }}</div>
            <div class="truncate text-xs text-muted-foreground">
              {{ folder.subtitle || formatTime(folder.updatedAt) }}
            </div>
          </button>
          <div v-if="folder.chatSessionId" class="shrink-0 self-start" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="14" class="text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" class="w-48">
                <DropdownMenuItem
                  @click="emit('convertToWorkspaceSession', folder.chatSessionId, folder.name)"
                >
                  <ArrowLeftFromLine :size="14" class="mr-2" />
                  {{ t('workspace.session.convertToWorkspace') }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <div v-if="folder.expanded" class="pb-1">
          <button
            v-for="run in folder.runs"
            :key="runKey(folder.taskId, run)"
            :data-testid="`background-run-${folder.taskId}-${run.runId ?? 'latest'}`"
            :class="
              cn(
                'flex w-full items-start gap-2 px-9 py-2 text-left transition-colors hover:bg-muted/50',
                isRunSelected(run.runId) && 'bg-muted',
              )
            "
            @click="run.runId && emit('selectRun', folder.taskId, run.runId)"
          >
            <component
              :is="normalizeStatusIcon(run.status)"
              :size="12"
              :class="
                cn(
                  'mt-0.5 shrink-0 text-muted-foreground',
                  run.status === 'running' && 'animate-spin text-primary',
                  run.status === 'completed' && 'text-green-500',
                )
              "
            />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm">{{ run.title }}</div>
              <div class="text-xs text-muted-foreground">
                {{ formatTime(run.updatedAt) }}
              </div>
            </div>
          </button>
          <button
            v-if="folder.runs.length === 0"
            class="w-full px-9 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/50"
            data-testid="background-run-empty"
            @click="emit('selectContainer', 'background_task', folder.taskId)"
          >
            No runs yet
          </button>
        </div>
      </div>

      <div
        class="px-3 pb-2 pt-4 text-[11px] font-medium uppercase tracking-wide text-muted-foreground"
      >
        External Channels
      </div>
      <div
        v-for="folder in externalFolders"
        :key="folder.containerId"
        :data-testid="`external-folder-${folder.containerId}`"
      >
        <div
          :class="
            cn(
              'flex items-start gap-2 px-3 py-2 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.containerId) && 'bg-muted',
            )
          "
        >
          <button
            class="mt-0.5 shrink-0 text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse external folder' : 'Expand external folder'"
            @click="emit('toggleExternalChannel', folder.containerId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="14" />
          </button>
          <Radio :size="14" class="mt-0.5 shrink-0 text-muted-foreground" />
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'external_channel', folder.containerId)"
          >
            <div class="truncate text-sm">{{ displayLabel(folder.name) }}</div>
            <div class="truncate text-xs text-muted-foreground">
              {{ sourceLabel(folder.sourceChannel) || folder.subtitle || formatTime(folder.updatedAt) }}
            </div>
          </button>
          <div v-if="folder.latestSessionId" class="shrink-0 self-start" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="14" class="text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" class="w-48">
                <DropdownMenuItem
                  @click="emit('rebuild', folder.latestSessionId, displayLabel(folder.name))"
                >
                  <RotateCcw :size="14" class="mr-2" />
                  {{ t('workspace.session.rebuild') }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <div v-if="folder.expanded" class="pb-1">
          <button
            v-for="run in folder.runs"
            :key="runKey(folder.containerId, run)"
            :data-testid="`external-run-${folder.containerId}-${run.runId ?? 'latest'}`"
            :class="
              cn(
                'flex w-full items-start gap-2 px-9 py-2 text-left transition-colors hover:bg-muted/50',
                isRunSelected(run.runId) && 'bg-muted',
              )
            "
            @click="run.runId && emit('selectRun', folder.containerId, run.runId)"
          >
            <component
              :is="normalizeStatusIcon(run.status)"
              :size="12"
              :class="
                cn(
                  'mt-0.5 shrink-0 text-muted-foreground',
                  run.status === 'running' && 'animate-spin text-primary',
                  run.status === 'completed' && 'text-green-500',
                )
              "
            />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm">{{ run.title }}</div>
              <div class="text-xs text-muted-foreground">
                {{ formatTime(run.updatedAt) }}
              </div>
            </div>
          </button>
          <button
            v-if="folder.runs.length === 0"
            class="w-full px-9 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/50"
            data-testid="external-run-empty"
            @click="emit('selectContainer', 'external_channel', folder.containerId)"
          >
            No runs yet
          </button>
        </div>
      </div>

      <div
        v-if="
          workspaceFolders.length === 0 &&
          backgroundFolders.length === 0 &&
          externalFolders.length === 0
        "
        data-testid="session-empty-state"
        class="px-3 py-6 text-sm text-muted-foreground"
      >
        {{ t('workspace.noSessions') }}
      </div>
    </div>
  </div>
</template>
