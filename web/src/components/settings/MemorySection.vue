<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
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

const agentId = ref('default')
const query = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const exporting = ref(false)

const stats = ref<MemoryStats | null>(null)
const sessions = ref<MemorySession[]>([])
const selectedSessionId = ref<string>('')
const chunks = ref<MemoryChunk[]>([])
const searchResults = ref<Array<{ id: string; score: number; content: string }>>([])
const exportMarkdown = ref('')

async function refreshOverview() {
  if (!agentId.value.trim()) {
    error.value = 'Agent ID is required.'
    return
  }

  loading.value = true
  error.value = null
  try {
    const [nextStats, nextSessions] = await Promise.all([
      getMemoryStats(agentId.value.trim()),
      listMemorySessions(agentId.value.trim()),
    ])
    stats.value = nextStats
    sessions.value = nextSessions
    if (nextSessions.length > 0 && !selectedSessionId.value) {
      selectedSessionId.value = nextSessions[0].id
      await loadSessionChunks(nextSessions[0].id)
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function runSearch() {
  if (!agentId.value.trim()) {
    error.value = 'Agent ID is required.'
    return
  }

  const searchQuery: MemorySearchQuery = {
    agent_id: agentId.value.trim(),
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

  loading.value = true
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
    loading.value = false
  }
}

async function loadSessionChunks(sessionId: string) {
  if (!sessionId) {
    chunks.value = []
    return
  }

  loading.value = true
  error.value = null
  try {
    chunks.value = await listMemoryChunksForSession(sessionId)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function exportAllMemory() {
  if (!agentId.value.trim()) {
    error.value = 'Agent ID is required.'
    return
  }

  exporting.value = true
  error.value = null
  try {
    const result = await exportMemoryMarkdown(agentId.value.trim())
    exportMarkdown.value = result.markdown
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    exporting.value = false
  }
}

onMounted(() => {
  refreshOverview()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">Memory</h2>
        <p class="text-muted-foreground">Inspect backend memory sessions, chunks, search results, and exports.</p>
      </div>
      <Button variant="outline" :disabled="loading" @click="refreshOverview">Refresh</Button>
    </div>

    <div class="rounded-lg border p-4 space-y-3">
      <label class="text-sm font-medium" for="memory-agent-id">Agent ID</label>
      <div class="flex items-center gap-2">
        <Input id="memory-agent-id" v-model="agentId" placeholder="default" />
        <Button :disabled="loading" @click="refreshOverview">Load</Button>
      </div>
    </div>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="rounded-lg border bg-card p-4 space-y-2">
        <h3 class="text-base font-semibold">Stats</h3>
        <div v-if="stats" class="space-y-1 text-sm">
          <p><span class="text-muted-foreground">agent:</span> {{ stats.agent_id }}</p>
          <p><span class="text-muted-foreground">sessions:</span> {{ stats.session_count }}</p>
          <p><span class="text-muted-foreground">chunks:</span> {{ stats.chunk_count }}</p>
          <p><span class="text-muted-foreground">tokens:</span> {{ stats.total_tokens }}</p>
        </div>
        <p v-else class="text-sm text-muted-foreground">No stats loaded.</p>
      </section>

      <section class="rounded-lg border bg-card p-4 space-y-3">
        <h3 class="text-base font-semibold">Search</h3>
        <div class="flex items-center gap-2">
          <Input
            v-model="query"
            placeholder="Search memory content"
            @keydown.enter.prevent="runSearch"
          />
          <Button :disabled="loading" @click="runSearch">Search</Button>
        </div>
        <div v-if="searchResults.length === 0" class="text-sm text-muted-foreground">
          No search results.
        </div>
        <div v-else class="space-y-2">
          <div
            v-for="result in searchResults"
            :key="result.id"
            class="rounded-md border p-2 text-sm"
          >
            <p class="text-xs text-muted-foreground">score: {{ result.score.toFixed(2) }} · id: {{ result.id }}</p>
            <p class="line-clamp-3">{{ result.content }}</p>
          </div>
        </div>
      </section>
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="rounded-lg border bg-card p-4 space-y-3">
        <h3 class="text-base font-semibold">Sessions</h3>
        <select
          v-model="selectedSessionId"
          class="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
          @change="loadSessionChunks(selectedSessionId)"
        >
          <option value="">Select a session</option>
          <option v-for="session in sessions" :key="session.id" :value="session.id">
            {{ session.name }} ({{ session.chunk_count }} chunks)
          </option>
        </select>

        <div v-if="chunks.length === 0" class="text-sm text-muted-foreground">
          No chunks loaded for selected session.
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
        <h3 class="text-base font-semibold">Export</h3>
        <Button :disabled="exporting" @click="exportAllMemory">Export Markdown</Button>
        <Textarea
          :model-value="exportMarkdown"
          rows="14"
          class="font-mono text-xs"
          placeholder="Exported markdown appears here"
          readonly
        />
      </section>
    </div>
  </div>
</template>
