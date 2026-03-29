<script setup lang="ts">
import { ref, computed } from 'vue'
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
  GitBranch,
  Search,
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
  ChildRunLoadState,
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
  toggleRunChildren: [containerId: string, runId: string]
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

interface FlattenedRunRow extends RunListItem {
  entryType: 'run'
  depth: number
  hasChildren: boolean
  isExpanded: boolean
  canToggleChildren: boolean
}

interface FlattenedRunPlaceholder {
  entryType: 'state'
  id: string
  depth: number
  containerId: string
  parentRunId: string
  state: Extract<ChildRunLoadState, 'loading' | 'error'>
  message: string
}

type FlattenedRunItem = FlattenedRunRow | FlattenedRunPlaceholder

const expandedRunKeys = ref<Set<string>>(new Set())
const searchQuery = ref('')

const filteredWorkspaceFolders = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return props.workspaceFolders
  return props.workspaceFolders.filter((f) => displayLabel(f.name).toLowerCase().includes(q))
})

const filteredBackgroundFolders = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return props.backgroundFolders
  return props.backgroundFolders.filter((f) => f.name.toLowerCase().includes(q))
})

const filteredExternalFolders = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return props.externalFolders
  return props.externalFolders.filter((f) => displayLabel(f.name).toLowerCase().includes(q))
})

function runContainsSelectedDescendant(run: RunListItem, selectedRunId: string | null | undefined): boolean {
  if (!selectedRunId) return false
  return (run.childRuns ?? []).some(
    (child) => child.runId === selectedRunId || runContainsSelectedDescendant(child, selectedRunId),
  )
}

function isRunExpanded(containerId: string, run: RunListItem): boolean {
  const key = runKey(containerId, run)
  return expandedRunKeys.value.has(key) || runContainsSelectedDescendant(run, props.currentRunId)
}

function toggleRunChildren(containerId: string, run: RunListItem) {
  if (!run.runId) return
  const key = runKey(containerId, run)
  const next = new Set(expandedRunKeys.value)
  if (next.has(key)) {
    next.delete(key)
  } else {
    next.add(key)
    emit('toggleRunChildren', containerId, run.runId)
  }
  expandedRunKeys.value = next
}

function retryLoadChildren(containerId: string, parentRunId: string) {
  // Emit directly without toggling expandedRunKeys so the folder stays open
  emit('toggleRunChildren', containerId, parentRunId)
}

function canToggleRunChildren(run: RunListItem): boolean {
  if (!run.runId) return false
  if (run.childRunsState === 'loaded') {
    return (run.childRuns?.length ?? 0) > 0
  }
  return true
}

function childPlaceholder(
  containerId: string,
  run: RunListItem,
  depth: number,
): FlattenedRunPlaceholder | null {
  const parentRunId = run.runId
  if (!parentRunId) return null

  if (run.childRunsState === 'loading') {
    return {
      entryType: 'state',
      id: `${parentRunId}-loading`,
      depth,
      containerId,
      parentRunId,
      state: 'loading',
      message: 'Loading child runs…',
    }
  }

  if (run.childRunsState === 'error') {
    return {
      entryType: 'state',
      id: `${parentRunId}-error`,
      depth,
      containerId,
      parentRunId,
      state: 'error',
      message: run.childRunsError || 'Failed to load child runs',
    }
  }

  return null
}

function flattenRuns(containerId: string, runs: RunListItem[], depth = 0): FlattenedRunItem[] {
  return runs.flatMap((run) => {
    const hasChildren = (run.childRuns?.length ?? 0) > 0
    const canToggleChildren = canToggleRunChildren(run)
    const expanded = isRunExpanded(containerId, run)
    const items: FlattenedRunItem[] = [
      {
        ...run,
        entryType: 'run',
        depth,
        hasChildren,
        isExpanded: expanded,
        canToggleChildren,
      },
    ]

    if (expanded) {
      const placeholder = childPlaceholder(containerId, run, depth + 1)
      if (placeholder) {
        items.push(placeholder)
      }
      if (hasChildren) {
        items.push(...flattenRuns(containerId, run.childRuns ?? [], depth + 1))
      }
    }

    return items
  })
}

function isRunSelected(runId: string | null | undefined): boolean {
  return !!runId && props.currentRunId === runId
}

function isContainerSelected(containerId: string): boolean {
  return props.currentContainerId === containerId && !props.currentRunId
}

function runHierarchyLabel(run: FlattenedRunRow): string {
  return run.depth > 0 ? 'Child run' : 'Run'
}

function runMetaLabel(run: FlattenedRunRow): string {
  return [runHierarchyLabel(run), run.agentName, formatTime(run.updatedAt)].filter(Boolean).join(' · ')
}

function runTitleClass(run: FlattenedRunRow): string {
  return run.depth > 0 ? 'text-[13px] text-foreground/85' : 'text-sm'
}
</script>

<template>
  <div class="flex min-h-0 flex-col bg-muted/30">
    <div class="space-y-1.5 px-3 pb-2 pt-2">
      <button
        class="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        data-testid="session-list-new-session"
        @click="emit('newSession')"
      >
        <Plus :size="13" />
        <span>{{ t('workspace.newSession') }}</span>
      </button>
      <div class="relative">
        <Search :size="11" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/60" />
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search..."
          class="w-full rounded-md border border-border/60 bg-background/60 py-1 pl-7 pr-3 text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-ring"
        />
      </div>
    </div>

    <div class="flex-1 overflow-auto py-1">
      <div
        v-for="folder in filteredWorkspaceFolders"
        :key="folder.containerId"
        :data-testid="`workspace-folder-${folder.containerId}`"
      >
        <div
          :class="
            cn(
              'group flex items-center gap-1.5 px-3 py-1.5 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.containerId) && 'bg-muted',
            )
          "
        >
          <button
            class="shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse workspace folder' : 'Expand workspace folder'"
            @click="emit('toggleWorkspaceFolder', folder.containerId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="12" />
          </button>
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'workspace', folder.containerId)"
          >
            <div class="truncate text-sm leading-snug">{{ displayLabel(folder.name) }}</div>
            <div class="flex items-center gap-1 text-[11px] text-muted-foreground/70">
              <span class="truncate">{{ folder.agentName || t('common.unknownAgent') }}</span>
              <span class="shrink-0 text-muted-foreground/40">·</span>
              <span class="shrink-0">{{ folder.subtitle || formatTime(folder.updatedAt) }}</span>
            </div>
          </button>
          <div class="shrink-0 opacity-0 transition-opacity group-hover:opacity-100" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="13" />
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
          <div
            v-for="run in flattenRuns(folder.containerId, folder.runs)"
            :key="run.entryType === 'run' ? runKey(folder.containerId, run) : run.id"
            :data-testid="
              run.entryType === 'run'
                ? `workspace-run-${folder.containerId}-${run.runId ?? 'latest'}`
                : `workspace-run-state-${folder.containerId}-${run.parentRunId}-${run.state}`
            "
            :data-run-depth="String(run.depth)"
            :class="
              cn(
                run.entryType === 'run'
                  ? 'flex items-start pr-3 transition-colors hover:bg-muted/50'
                  : 'flex items-start pr-3 text-xs text-muted-foreground',
                run.entryType === 'run' && isRunSelected(run.runId) && 'bg-muted',
                run.depth > 0 && 'bg-muted/20 hover:bg-muted/40',
              )
            "
            :style="{ paddingLeft: `${1.5 + run.depth * 1.25}rem` }"
          >
            <template v-if="run.entryType === 'run'">
              <button
                v-if="run.canToggleChildren"
                :data-testid="`workspace-run-toggle-${folder.containerId}-${run.runId ?? 'latest'}`"
                class="mt-1.5 shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
                :aria-label="run.isExpanded ? 'Collapse child runs' : 'Expand child runs'"
                @click.stop="toggleRunChildren(folder.containerId, run)"
              >
                <component :is="run.isExpanded ? ChevronDown : ChevronRight" :size="11" />
              </button>
              <div v-else class="w-3 shrink-0" />
              <button
                class="flex min-w-0 flex-1 items-start gap-2 py-1 text-left"
                @click="run.runId && emit('selectRun', folder.containerId, run.runId)"
              >
                <GitBranch
                  v-if="run.depth > 0"
                  :size="11"
                  class="mt-0.5 shrink-0 text-muted-foreground/70"
                />
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
                  <div class="flex items-center gap-1.5">
                    <div class="truncate font-medium" :class="runTitleClass(run)">{{ run.title }}</div>
                    <span
                      v-if="run.depth > 0"
                      class="shrink-0 rounded-sm border border-border/60 bg-muted/50 px-1 py-0 text-[8px] font-medium uppercase tracking-[0.08em] text-muted-foreground"
                    >
                      Child
                    </span>
                  </div>
                  <div class="text-[11px] text-muted-foreground/90">
                    {{ runMetaLabel(run) }}
                  </div>
                </div>
              </button>
            </template>
            <div v-else class="flex min-w-0 flex-1 items-center justify-between gap-2 py-2">
              <div class="flex min-w-0 items-center gap-2">
                <Loader2
                  v-if="run.state === 'loading'"
                  :size="12"
                  class="shrink-0 animate-spin text-muted-foreground"
                />
                <Activity
                  v-else
                  :size="12"
                  class="shrink-0 text-destructive"
                />
                <span class="truncate" :class="run.state === 'error' ? 'text-destructive' : ''">{{ run.message }}</span>
              </div>
              <button
                v-if="run.state === 'error'"
                class="shrink-0 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground"
                @click.stop="retryLoadChildren(run.containerId, run.parentRunId)"
              >
                <RotateCcw :size="10" />
                Retry
              </button>
            </div>
          </div>
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

      <div class="mx-3 mb-1 mt-3 border-t border-border/40" />
      <div class="px-3 pb-1 pt-2 text-[10px] font-medium uppercase tracking-widest text-muted-foreground/50">
        Background Agents
      </div>
      <div
        v-for="folder in filteredBackgroundFolders"
        :key="folder.taskId"
        :data-testid="`background-folder-${folder.taskId}`"
      >
        <div
          :class="
            cn(
              'group flex items-center gap-1.5 px-3 py-1.5 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.taskId) && 'bg-muted',
            )
          "
        >
          <button
            class="shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse background folder' : 'Expand background folder'"
            @click="emit('toggleBackgroundTask', folder.taskId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="12" />
          </button>
          <component
            :is="normalizeStatusIcon(folder.status)"
            :size="12"
            :class="
              cn(
                'shrink-0 text-muted-foreground',
                folder.status === 'running' && 'animate-spin text-primary',
                folder.status === 'completed' && 'text-green-500',
              )
            "
          />
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'background_task', folder.taskId)"
          >
            <div class="truncate text-sm leading-snug">{{ folder.name }}</div>
            <div class="text-[11px] text-muted-foreground/70">
              {{ folder.subtitle || formatTime(folder.updatedAt) }}
            </div>
          </button>
          <div v-if="folder.chatSessionId" class="shrink-0 opacity-0 transition-opacity group-hover:opacity-100" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="13" />
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
          <div
            v-for="run in flattenRuns(folder.taskId, folder.runs)"
            :key="run.entryType === 'run' ? runKey(folder.taskId, run) : run.id"
            :data-testid="
              run.entryType === 'run'
                ? `background-run-${folder.taskId}-${run.runId ?? 'latest'}`
                : `background-run-state-${folder.taskId}-${run.parentRunId}-${run.state}`
            "
            :data-run-depth="String(run.depth)"
            :class="
              cn(
                run.entryType === 'run'
                  ? 'flex items-start pr-3 transition-colors hover:bg-muted/50'
                  : 'flex items-start pr-3 text-xs text-muted-foreground',
                run.entryType === 'run' && isRunSelected(run.runId) && 'bg-muted',
                run.depth > 0 && 'bg-muted/20 hover:bg-muted/40',
              )
            "
            :style="{ paddingLeft: `${1.5 + run.depth * 1.25}rem` }"
          >
            <template v-if="run.entryType === 'run'">
              <button
                v-if="run.canToggleChildren"
                :data-testid="`background-run-toggle-${folder.taskId}-${run.runId ?? 'latest'}`"
                class="mt-1.5 shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
                :aria-label="run.isExpanded ? 'Collapse child runs' : 'Expand child runs'"
                @click.stop="toggleRunChildren(folder.taskId, run)"
              >
                <component :is="run.isExpanded ? ChevronDown : ChevronRight" :size="11" />
              </button>
              <div v-else class="w-3 shrink-0" />
              <button
                class="flex min-w-0 flex-1 items-start gap-2 py-1 text-left"
                @click="run.runId && emit('selectRun', folder.taskId, run.runId)"
              >
                <GitBranch
                  v-if="run.depth > 0"
                  :size="11"
                  class="mt-0.5 shrink-0 text-muted-foreground/70"
                />
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
                  <div class="flex items-center gap-1.5">
                    <div class="truncate font-medium" :class="runTitleClass(run)">{{ run.title }}</div>
                    <span
                      v-if="run.depth > 0"
                      class="shrink-0 rounded-sm border border-border/60 bg-muted/50 px-1 py-0 text-[8px] font-medium uppercase tracking-[0.08em] text-muted-foreground"
                    >
                      Child
                    </span>
                  </div>
                  <div class="text-[11px] text-muted-foreground/90">
                    {{ runMetaLabel(run) }}
                  </div>
                </div>
              </button>
            </template>
            <div v-else class="flex min-w-0 flex-1 items-center justify-between gap-2 py-2">
              <div class="flex min-w-0 items-center gap-2">
                <Loader2
                  v-if="run.state === 'loading'"
                  :size="12"
                  class="shrink-0 animate-spin text-muted-foreground"
                />
                <Activity
                  v-else
                  :size="12"
                  class="shrink-0 text-destructive"
                />
                <span class="truncate" :class="run.state === 'error' ? 'text-destructive' : ''">{{ run.message }}</span>
              </div>
              <button
                v-if="run.state === 'error'"
                class="shrink-0 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground"
                @click.stop="retryLoadChildren(run.containerId, run.parentRunId)"
              >
                <RotateCcw :size="10" />
                Retry
              </button>
            </div>
          </div>
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

      <div class="mx-3 mb-1 mt-3 border-t border-border/40" />
      <div class="px-3 pb-1 pt-2 text-[10px] font-medium uppercase tracking-widest text-muted-foreground/50">
        External Channels
      </div>
      <div
        v-for="folder in filteredExternalFolders"
        :key="folder.containerId"
        :data-testid="`external-folder-${folder.containerId}`"
      >
        <div
          :class="
            cn(
              'group flex items-center gap-1.5 px-3 py-1.5 transition-colors hover:bg-muted/50',
              isContainerSelected(folder.containerId) && 'bg-muted',
            )
          "
        >
          <button
            class="shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
            :aria-label="folder.expanded ? 'Collapse external folder' : 'Expand external folder'"
            @click="emit('toggleExternalChannel', folder.containerId)"
          >
            <component :is="folder.expanded ? ChevronDown : ChevronRight" :size="12" />
          </button>
          <Radio :size="12" class="shrink-0 text-muted-foreground/70" />
          <button
            class="min-w-0 flex-1 text-left"
            @click="emit('selectContainer', 'external_channel', folder.containerId)"
          >
            <div class="truncate text-sm leading-snug">{{ displayLabel(folder.name) }}</div>
            <div class="flex items-center gap-1 text-[11px] text-muted-foreground/70">
              <span class="truncate">{{ sourceLabel(folder.sourceChannel) || folder.subtitle || formatTime(folder.updatedAt) }}</span>
            </div>
          </button>
          <div v-if="folder.latestSessionId" class="shrink-0 opacity-0 transition-opacity group-hover:opacity-100" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted-foreground/10 hover:text-foreground"
                >
                  <MoreHorizontal :size="13" />
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
          <div
            v-for="run in flattenRuns(folder.containerId, folder.runs)"
            :key="run.entryType === 'run' ? runKey(folder.containerId, run) : run.id"
            :data-testid="
              run.entryType === 'run'
                ? `external-run-${folder.containerId}-${run.runId ?? 'latest'}`
                : `external-run-state-${folder.containerId}-${run.parentRunId}-${run.state}`
            "
            :data-run-depth="String(run.depth)"
            :class="
              cn(
                run.entryType === 'run'
                  ? 'flex items-start pr-3 transition-colors hover:bg-muted/50'
                  : 'flex items-start pr-3 text-xs text-muted-foreground',
                run.entryType === 'run' && isRunSelected(run.runId) && 'bg-muted',
                run.depth > 0 && 'bg-muted/20 hover:bg-muted/40',
              )
            "
            :style="{ paddingLeft: `${1.5 + run.depth * 1.25}rem` }"
          >
            <template v-if="run.entryType === 'run'">
              <button
                v-if="run.canToggleChildren"
                :data-testid="`external-run-toggle-${folder.containerId}-${run.runId ?? 'latest'}`"
                class="mt-1.5 shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
                :aria-label="run.isExpanded ? 'Collapse child runs' : 'Expand child runs'"
                @click.stop="toggleRunChildren(folder.containerId, run)"
              >
                <component :is="run.isExpanded ? ChevronDown : ChevronRight" :size="11" />
              </button>
              <div v-else class="w-3 shrink-0" />
              <button
                class="flex min-w-0 flex-1 items-start gap-2 py-1 text-left"
                @click="run.runId && emit('selectRun', folder.containerId, run.runId)"
              >
                <GitBranch
                  v-if="run.depth > 0"
                  :size="11"
                  class="mt-0.5 shrink-0 text-muted-foreground/70"
                />
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
                  <div class="flex items-center gap-1.5">
                    <div class="truncate font-medium" :class="runTitleClass(run)">{{ run.title }}</div>
                    <span
                      v-if="run.depth > 0"
                      class="shrink-0 rounded-sm border border-border/60 bg-muted/50 px-1 py-0 text-[8px] font-medium uppercase tracking-[0.08em] text-muted-foreground"
                    >
                      Child
                    </span>
                  </div>
                  <div class="text-[11px] text-muted-foreground/90">
                    {{ runMetaLabel(run) }}
                  </div>
                </div>
              </button>
            </template>
            <div v-else class="flex min-w-0 flex-1 items-center justify-between gap-2 py-2">
              <div class="flex min-w-0 items-center gap-2">
                <Loader2
                  v-if="run.state === 'loading'"
                  :size="12"
                  class="shrink-0 animate-spin text-muted-foreground"
                />
                <Activity
                  v-else
                  :size="12"
                  class="shrink-0 text-destructive"
                />
                <span class="truncate" :class="run.state === 'error' ? 'text-destructive' : ''">{{ run.message }}</span>
              </div>
              <button
                v-if="run.state === 'error'"
                class="shrink-0 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground"
                @click.stop="retryLoadChildren(run.containerId, run.parentRunId)"
              >
                <RotateCcw :size="10" />
                Retry
              </button>
            </div>
          </div>
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
        v-if="searchQuery && filteredWorkspaceFolders.length === 0 && filteredBackgroundFolders.length === 0 && filteredExternalFolders.length === 0"
        class="px-3 py-6 text-center text-xs text-muted-foreground"
      >
        No sessions match "{{ searchQuery }}"
      </div>
      <div
        v-else-if="
          !searchQuery &&
          workspaceFolders.length === 0 &&
          backgroundFolders.length === 0 &&
          externalFolders.length === 0
        "
        data-testid="session-empty-state"
        class="px-3 py-8 text-center"
      >
        <MessageSquare :size="28" class="mx-auto mb-2 text-muted-foreground/40" />
        <p class="mb-1 text-sm font-medium text-muted-foreground">{{ t('workspace.noSessions') }}</p>
        <p class="mb-4 text-xs text-muted-foreground/70">Create a session to get started</p>
        <Button
          variant="outline"
          size="sm"
          class="gap-1.5"
          data-testid="session-empty-new-session"
          @click="emit('newSession')"
        >
          <Plus :size="13" />
          New Session
        </Button>
      </div>
    </div>
  </div>
</template>
