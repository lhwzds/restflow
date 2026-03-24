<script setup lang="ts">
import {
  Plus,
  MessageSquare,
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
import type { BackgroundTaskFolder, SessionItem } from '@/types/workspace'
import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'

const props = withDefaults(defineProps<{
  sessions: SessionItem[]
  currentSessionId: string | null
  backgroundFolders?: BackgroundTaskFolder[]
  currentBackgroundTaskId?: string | null
  currentBackgroundRunId?: string | null
}>(), {
  backgroundFolders: () => [],
  currentBackgroundTaskId: null,
  currentBackgroundRunId: null,
})

const { t } = useI18n()

const emit = defineEmits<{
  select: [id: string]
  newSession: []
  rename: [id: string, currentName: string]
  archive: [id: string, name: string]
  delete: [id: string, name: string]
  convertToBackgroundAgent: [id: string, name: string]
  convertToWorkspaceSession: [id: string, name: string]
  viewRunTrace: [taskId: string]
  rebuild: [id: string, name: string]
  toggleBackgroundTask: [taskId: string]
  selectBackgroundRun: [taskId: string, runId: string | null]
}>()

const DISPLAY_PREFIXES = ['channel:', 'background:']

function displaySessionName(session: SessionItem): string {
  const trimmedName = session.name.trimStart()
  const normalized = trimmedName.toLowerCase()

  for (const prefix of DISPLAY_PREFIXES) {
    if (!normalized.startsWith(prefix)) {
      continue
    }

    const displayName = trimmedName.slice(prefix.length).trim()
    return displayName || session.name
  }

  return session.name
}

function isExternallyManagedSession(session: SessionItem): boolean {
  return !!session.sourceChannel && session.sourceChannel !== 'workspace'
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

function sessionTagLabel(session: SessionItem): string | null {
  if (session.isBackgroundSession) {
    return t('workspace.background')
  }

  return sourceLabel(session.sourceChannel)
}

function hasSessionTag(session: SessionItem): boolean {
  return sessionTagLabel(session) !== null
}

const formatTime = (timestamp: number) => {
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

function backgroundRunKey(taskId: string, runId: string | null | undefined): string {
  return `${taskId}:${runId ?? 'latest'}`
}
</script>

<template>
  <div class="flex flex-col min-h-0 bg-muted/30">
    <!-- Header -->
    <div class="px-3 pt-2 pb-3 space-y-2">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('newSession')">
        <Plus :size="16" />
        <span>{{ t('workspace.newSession') }}</span>
      </Button>
    </div>

    <!-- Session List -->
    <div class="flex-1 overflow-auto py-2">
      <div class="px-3 pb-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Workspace Sessions
      </div>
      <div
        v-for="session in sessions"
        :key="session.id"
        :data-testid="`session-row-${session.id}`"
        :class="
          cn(
            'group relative w-full cursor-pointer px-3 py-2 text-left transition-colors hover:bg-muted/50',
            currentSessionId === session.id && 'bg-muted',
          )
        "
        @click="emit('select', session.id)"
      >
        <div class="flex items-start gap-2">
          <div class="mt-0.5">
            <Loader2
              v-if="session.status === 'running'"
              :size="14"
              class="animate-spin text-primary"
            />
            <Check v-else-if="session.status === 'completed'" :size="14" class="text-green-500" />
            <MessageSquare v-else :size="14" class="text-muted-foreground" />
          </div>

          <div class="flex-1 min-w-0">
            <div class="text-sm truncate">{{ displaySessionName(session) }}</div>
            <div class="text-xs text-muted-foreground truncate">
              <span
                v-if="sessionTagLabel(session)"
                class="inline-flex items-center rounded border border-border px-1 py-0 text-[10px] uppercase tracking-wide"
              >
                {{ sessionTagLabel(session) }}
              </span>
              <span v-if="session.agentName">
                <span v-if="hasSessionTag(session)"> · </span>
                {{ session.agentName }}
              </span>
              <span v-else-if="!hasSessionTag(session)">
                {{ t('common.unknownAgent') }}
              </span>
            </div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(session.updatedAt) }}
            </div>
          </div>

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
                <template v-if="isExternallyManagedSession(session)">
                  <DropdownMenuItem
                    @click="emit('rebuild', session.id, displaySessionName(session))"
                  >
                    <RotateCcw :size="14" class="mr-2" />
                    {{ t('workspace.session.rebuild') }}
                  </DropdownMenuItem>
                </template>
                <template v-else>
                  <DropdownMenuItem
                    @click="emit('rename', session.id, displaySessionName(session))"
                  >
                    <Pencil :size="14" class="mr-2" />
                    {{ t('workspace.session.rename') }}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    v-if="session.isBackgroundSession && session.backgroundTaskId"
                    @click="emit('viewRunTrace', session.backgroundTaskId)"
                  >
                    <Activity :size="14" class="mr-2" />
                    {{ t('workspace.session.viewRunTrace') }}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    v-if="session.isBackgroundSession"
                    @click="
                      emit('convertToWorkspaceSession', session.id, displaySessionName(session))
                    "
                  >
                    <ArrowLeftFromLine :size="14" class="mr-2" />
                    {{ t('workspace.session.convertToWorkspace') }}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    v-else
                    @click="
                      emit('convertToBackgroundAgent', session.id, displaySessionName(session))
                    "
                  >
                    <ArrowRightFromLine :size="14" class="mr-2" />
                    {{ t('workspace.session.convertToBackground') }}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    @click="emit('archive', session.id, displaySessionName(session))"
                  >
                    <Archive :size="14" class="mr-2" />
                    {{ t('workspace.session.archive') }}
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    class="text-destructive focus:text-destructive"
                    @click="emit('delete', session.id, displaySessionName(session))"
                  >
                    <Trash2 :size="14" class="mr-2" />
                    {{ t('workspace.session.delete') }}
                  </DropdownMenuItem>
                </template>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
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
        class="border-y border-transparent"
      >
        <button
          class="flex w-full items-start gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50"
          @click="emit('toggleBackgroundTask', folder.taskId)"
        >
          <component
            :is="folder.expanded ? ChevronDown : ChevronRight"
            :size="14"
            class="mt-0.5 shrink-0 text-muted-foreground"
          />
          <Loader2
            v-if="folder.status === 'running'"
            :size="14"
            class="mt-0.5 shrink-0 animate-spin text-primary"
          />
          <Check
            v-else-if="folder.status === 'completed'"
            :size="14"
            class="mt-0.5 shrink-0 text-green-500"
          />
          <Activity v-else :size="14" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <div class="truncate text-sm">{{ folder.name }}</div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(folder.updatedAt) }}
            </div>
          </div>
        </button>

        <div v-if="folder.expanded" class="pb-1">
          <button
            v-for="run in folder.runs"
            :key="backgroundRunKey(folder.taskId, run.runId)"
            :data-testid="`background-run-${folder.taskId}-${run.runId ?? 'latest'}`"
            :class="
              cn(
                'flex w-full items-start gap-2 px-9 py-2 text-left transition-colors hover:bg-muted/50',
                currentBackgroundTaskId === folder.taskId &&
                  currentBackgroundRunId === (run.runId ?? null) &&
                  'bg-muted',
              )
            "
            @click="emit('selectBackgroundRun', folder.taskId, run.runId ?? null)"
          >
            <Loader2
              v-if="run.status === 'running'"
              :size="12"
              class="mt-0.5 shrink-0 animate-spin text-primary"
            />
            <Check
              v-else-if="run.status === 'completed'"
              :size="12"
              class="mt-0.5 shrink-0 text-green-500"
            />
            <Activity v-else :size="12" class="mt-0.5 shrink-0 text-muted-foreground" />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm">{{ run.title }}</div>
              <div class="text-xs text-muted-foreground">
                {{ formatTime(run.updatedAt) }}
              </div>
            </div>
          </button>
          <div
            v-if="folder.runs.length === 0"
            class="px-9 py-2 text-xs text-muted-foreground"
            data-testid="background-run-empty"
          >
            No runs yet
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div
        v-if="sessions.length === 0 && backgroundFolders.length === 0"
        data-testid="session-empty-state"
        class="px-3 py-8 text-center text-sm text-muted-foreground"
      >
        {{ t('workspace.noSessions') }}
      </div>
    </div>
  </div>
</template>
