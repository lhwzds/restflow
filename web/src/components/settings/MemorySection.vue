<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
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
import { Textarea } from '@/components/ui/textarea'
import {
  deleteMemorySession,
  exportMemoryMarkdown,
  getMemoryStats,
  listMemoryChunksForSession,
  listMemorySessions,
  searchMemory,
  type MemoryChunk,
  type MemorySearchQuery,
  type MemorySession,
  type MemoryStats,
} from '@/api/memory'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()

const agentId = ref('default')
const query = ref('')

const loadingOverview = ref(false)
const loadingSearch = ref(false)
const loadingChunks = ref(false)
const exporting = ref(false)
const deletingSession = ref(false)
const error = ref<string | null>(null)

const stats = ref<MemoryStats | null>(null)
const sessions = ref<MemorySession[]>([])
const selectedSessionId = ref<string>('')
const chunks = ref<MemoryChunk[]>([])
const searchResults = ref<Array<{ id: string; score: number; content: string }>>([])
const exportMarkdown = ref('')
const exportFilename = ref('memory-export.md')

function getNormalizedAgentId(): string | null {
  const trimmed = agentId.value.trim()
  if (!trimmed) {
    error.value = t('settings.memory.agentRequired')
    return null
  }
  return trimmed
}

async function refreshOverview() {
  const normalizedAgentId = getNormalizedAgentId()
  if (!normalizedAgentId) return

  loadingOverview.value = true
  error.value = null
  try {
    const [nextStats, nextSessions] = await Promise.all([
      getMemoryStats(normalizedAgentId),
      listMemorySessions(normalizedAgentId),
    ])
    stats.value = nextStats
    sessions.value = nextSessions

    if (nextSessions.length === 0) {
      selectedSessionId.value = ''
      chunks.value = []
      return
    }

    const firstSession = nextSessions[0]
    if (!firstSession) {
      selectedSessionId.value = ''
      chunks.value = []
      return
    }

    const nextSessionId = nextSessions.some((item) => item.id === selectedSessionId.value)
      ? selectedSessionId.value
      : firstSession.id
    selectedSessionId.value = nextSessionId
    await loadSessionChunks(nextSessionId)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loadingOverview.value = false
  }
}

async function runSearch() {
  const normalizedAgentId = getNormalizedAgentId()
  if (!normalizedAgentId) return

  const searchQuery: MemorySearchQuery = {
    agent_id: normalizedAgentId,
    query: query.value.trim() || null,
    search_mode: 'keyword',
    session_id: selectedSessionId.value || null,
    tags: [],
    source_type: null,
    from_time: null,
    to_time: null,
    limit: 20,
    offset: 0,
  }

  loadingSearch.value = true
  error.value = null
  try {
    const result = await searchMemory(searchQuery)
    searchResults.value = result.chunks.map((chunk) => ({
      id: chunk.chunk.id,
      score: chunk.score,
      content: chunk.chunk.content,
    }))
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loadingSearch.value = false
  }
}

async function loadSessionChunks(sessionId: string) {
  if (!sessionId) {
    chunks.value = []
    return
  }

  loadingChunks.value = true
  error.value = null
  try {
    chunks.value = await listMemoryChunksForSession(sessionId)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loadingChunks.value = false
  }
}

async function handleSessionChange(sessionId: string) {
  selectedSessionId.value = sessionId
  await loadSessionChunks(sessionId)
}

async function handleDeleteSession() {
  if (!selectedSessionId.value) return

  const selected = sessions.value.find((session) => session.id === selectedSessionId.value)
  if (!selected) return

  const confirmed = await confirm({
    title: t('settings.memory.deleteSessionConfirmTitle'),
    description: t('settings.memory.deleteSessionConfirmDescription', { name: selected.name }),
    confirmText: t('settings.memory.deleteSession'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })
  if (!confirmed) return

  deletingSession.value = true
  error.value = null
  try {
    await deleteMemorySession(selected.id, true)
    toast.success(t('settings.memory.deleteSessionSuccess'))
    await refreshOverview()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    deletingSession.value = false
  }
}

async function exportAllMemory() {
  const normalizedAgentId = getNormalizedAgentId()
  if (!normalizedAgentId) return

  exporting.value = true
  error.value = null
  try {
    const result = await exportMemoryMarkdown(normalizedAgentId)
    exportMarkdown.value = result.markdown
    exportFilename.value = result.suggested_filename
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    exporting.value = false
  }
}

async function copyExport() {
  if (!exportMarkdown.value.trim()) return
  try {
    await navigator.clipboard.writeText(exportMarkdown.value)
    toast.success(t('settings.memory.copied'))
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

function downloadExport() {
  if (!exportMarkdown.value.trim()) return

  const blob = new Blob([exportMarkdown.value], { type: 'text/markdown;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = exportFilename.value || 'memory-export.md'
  document.body.appendChild(anchor)
  anchor.click()
  document.body.removeChild(anchor)
  URL.revokeObjectURL(url)
  toast.success(t('settings.memory.downloaded'))
}

onMounted(() => {
  refreshOverview()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">{{ t('settings.memory.title') }}</h2>
        <p class="text-muted-foreground">{{ t('settings.memory.description') }}</p>
      </div>
      <Button variant="outline" :disabled="loadingOverview" @click="refreshOverview">
        {{ t('settings.memory.refresh') }}
      </Button>
    </div>

    <div class="rounded-lg border p-4 space-y-3">
      <Label for="memory-agent-id">{{ t('settings.memory.agentIdLabel') }}</Label>
      <div class="flex items-center gap-2">
        <Input
          id="memory-agent-id"
          v-model="agentId"
          :placeholder="t('settings.memory.agentIdPlaceholder')"
          @keydown.enter.prevent="refreshOverview"
        />
        <Button :disabled="loadingOverview" @click="refreshOverview">{{ t('settings.memory.load') }}</Button>
      </div>
    </div>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="rounded-lg border bg-card p-4 space-y-2">
        <h3 class="text-base font-semibold">{{ t('settings.memory.statsTitle') }}</h3>
        <div v-if="loadingOverview" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.memory.refresh') }}
        </div>
        <div v-else-if="stats" class="space-y-1 text-sm">
          <p><span class="text-muted-foreground">{{ t('settings.memory.agent') }}:</span> {{ stats.agent_id }}</p>
          <p><span class="text-muted-foreground">{{ t('settings.memory.sessions') }}:</span> {{ stats.session_count }}</p>
          <p><span class="text-muted-foreground">{{ t('settings.memory.chunks') }}:</span> {{ stats.chunk_count }}</p>
          <p><span class="text-muted-foreground">{{ t('settings.memory.tokens') }}:</span> {{ stats.total_tokens }}</p>
        </div>
        <p v-else class="text-sm text-muted-foreground">{{ t('settings.memory.noStats') }}</p>
      </section>

      <section class="rounded-lg border bg-card p-4 space-y-3">
        <h3 class="text-base font-semibold">{{ t('settings.memory.searchTitle') }}</h3>
        <div class="flex items-center gap-2">
          <Input
            v-model="query"
            :placeholder="t('settings.memory.searchPlaceholder')"
            @keydown.enter.prevent="runSearch"
          />
          <Button :disabled="loadingSearch" @click="runSearch">{{ t('settings.memory.search') }}</Button>
        </div>
        <div v-if="loadingSearch" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.memory.search') }}
        </div>
        <div v-else-if="searchResults.length === 0" class="text-sm text-muted-foreground">
          {{ t('settings.memory.noSearchResults') }}
        </div>
        <div v-else class="space-y-2">
          <div
            v-for="result in searchResults"
            :key="result.id"
            class="rounded-md border p-2 text-sm"
          >
            <p class="text-xs text-muted-foreground">
              {{ t('settings.memory.score') }}: {{ result.score.toFixed(2) }} · id: {{ result.id }}
            </p>
            <p class="line-clamp-3">{{ result.content }}</p>
          </div>
        </div>
      </section>
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="rounded-lg border bg-card p-4 space-y-3">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-base font-semibold">{{ t('settings.memory.sessionsTitle') }}</h3>
          <Button
            variant="destructive"
            size="sm"
            :disabled="!selectedSessionId || deletingSession"
            @click="handleDeleteSession"
          >
            {{ t('settings.memory.deleteSession') }}
          </Button>
        </div>

        <Select
          v-model="selectedSessionId"
          :disabled="sessions.length === 0"
          @update:model-value="(value) => handleSessionChange(String(value))"
        >
          <SelectTrigger>
            <SelectValue :placeholder="t('settings.memory.selectSession')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="session in sessions" :key="session.id" :value="session.id">
              {{ session.name }} ({{ session.chunk_count }})
            </SelectItem>
          </SelectContent>
        </Select>

        <div v-if="loadingChunks" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.memory.sessionsTitle') }}
        </div>
        <div v-else-if="chunks.length === 0" class="text-sm text-muted-foreground">
          {{ t('settings.memory.noChunks') }}
        </div>
        <div v-else class="max-h-72 space-y-2 overflow-auto pr-1">
          <div
            v-for="chunk in chunks"
            :key="chunk.id"
            class="rounded-md border p-2"
          >
            <p class="text-xs text-muted-foreground">chunk: {{ chunk.id }} · tokens: {{ chunk.token_count ?? 0 }}</p>
            <p class="line-clamp-3 text-sm">{{ chunk.content }}</p>
          </div>
        </div>
      </section>

      <section class="rounded-lg border bg-card p-4 space-y-3">
        <h3 class="text-base font-semibold">{{ t('settings.memory.exportTitle') }}</h3>
        <div class="flex items-center gap-2">
          <Button :disabled="exporting" @click="exportAllMemory">{{ t('settings.memory.exportMarkdown') }}</Button>
          <Button variant="outline" :disabled="!exportMarkdown" @click="copyExport">
            {{ t('settings.memory.copy') }}
          </Button>
          <Button variant="outline" :disabled="!exportMarkdown" @click="downloadExport">
            {{ t('settings.memory.download') }}
          </Button>
        </div>
        <Textarea
          :model-value="exportMarkdown"
          rows="14"
          class="font-mono text-xs"
          :placeholder="t('settings.memory.exportPlaceholder')"
          readonly
        />
      </section>
    </div>
  </div>
</template>
