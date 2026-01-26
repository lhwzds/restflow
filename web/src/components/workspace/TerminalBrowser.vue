<!--
  TerminalBrowser Component - Design Decisions:

  1. DASHED BORDER on "New Terminal" card: Unlike FileBrowser, terminal items are
     displayed as Card components with borders. The "New Terminal" button uses a
     dashed border Card to match this visual style and maintain consistency.
     (Compare with FileBrowser which uses no border because file items have no borders)

  2. searchQuery and viewMode are PROPS, not local state: These controls are managed
     in the parent (SkillWorkspace) and displayed in the header for a cleaner UI.

  3. Auto-restart stopped sessions: When clicking a stopped terminal, it automatically
     restarts the PTY session for better UX.
-->
<script setup lang="ts">
import { Terminal, Plus, Trash2, Loader2 } from 'lucide-vue-next'
import { ref, computed } from 'vue'
import { useEditorTabs, type EditorTab } from '@/composables/editor/useEditorTabs'
import { useTerminalSessions, type TerminalSession } from '@/composables/editor/useTerminalSessions'
import { closePty } from '@/api/pty'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'

const props = defineProps<{
  searchQuery: string
  viewMode: 'grid' | 'list'
}>()

const emit = defineEmits<{
  open: [tab: EditorTab]
}>()

const { openTerminal, closeTab } = useEditorTabs()
const { sessions, isLoading, createSession, deleteSession, restartSession } = useTerminalSessions()

const isCreating = ref(false)
const deletingIds = ref<Set<string>>(new Set())

// Filter sessions by search query
const filteredSessions = computed(() => {
  if (!props.searchQuery) return sessions.value
  const query = props.searchQuery.toLowerCase()
  return sessions.value.filter((session) => session.name.toLowerCase().includes(query))
})

// Create a new terminal and open it
const handleCreateTerminal = async () => {
  if (isCreating.value) return

  isCreating.value = true
  try {
    const session = await createSession()
    const tab = openTerminal(session)
    emit('open', tab)
  } catch (error) {
    console.error('Failed to create terminal:', error)
  } finally {
    isCreating.value = false
  }
}

const openingIds = ref<Set<string>>(new Set())

// Open an existing terminal session (auto-restart if stopped)
const handleOpenSession = async (session: TerminalSession) => {
  if (openingIds.value.has(session.id)) return

  let sessionToOpen = session

  // Auto-restart stopped sessions
  if (session.status === 'stopped') {
    openingIds.value.add(session.id)
    try {
      sessionToOpen = await restartSession(session.id)
    } catch (error) {
      console.error('Failed to restart terminal:', error)
      openingIds.value.delete(session.id)
      return
    }
    openingIds.value.delete(session.id)
  }

  const tab = openTerminal(sessionToOpen)
  emit('open', tab)
}

// Delete a terminal session
const handleDeleteSession = async (event: Event, session: TerminalSession) => {
  event.stopPropagation()

  if (deletingIds.value.has(session.id)) return

  deletingIds.value.add(session.id)
  try {
    // Close the tab if it's open
    closeTab(session.id)
    // Close PTY if running
    if (session.status === 'running') {
      try {
        await closePty(session.id)
      } catch {
        // PTY might already be closed, ignore
      }
    }
    // Delete the session
    await deleteSession(session.id)
  } catch (error) {
    console.error('Failed to delete terminal:', error)
  } finally {
    deletingIds.value.delete(session.id)
  }
}

// Format date for display
const formatDate = (timestamp: number) => {
  const date = new Date(timestamp)
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Content Area -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Loading state -->
      <div
        v-if="isLoading"
        class="flex flex-col items-center justify-center h-full text-muted-foreground"
      >
        <Loader2 :size="32" class="mb-2 animate-spin" />
        <span class="text-sm">Loading...</span>
      </div>

      <!-- Grid View -->
      <div
        v-else-if="viewMode === 'grid'"
        class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4"
      >
        <!-- Existing terminal sessions -->
        <Card
          v-for="session in filteredSessions"
          :key="session.id"
          class="group relative cursor-pointer hover:border-primary transition-colors"
          :class="{ 'opacity-50': deletingIds.has(session.id) || openingIds.has(session.id) }"
          @click="handleOpenSession(session)"
        >
          <CardContent class="flex flex-col items-center justify-center p-6">
            <!-- Status indicator -->
            <div class="absolute top-2 left-2">
              <span
                v-if="session.status === 'running'"
                class="h-2 w-2 rounded-full bg-green-500 inline-block animate-pulse"
                title="Running"
              />
              <span v-else class="h-2 w-2 rounded-full bg-gray-400 inline-block" title="Stopped" />
            </div>
            <!-- Loading spinner when opening -->
            <Loader2
              v-if="openingIds.has(session.id)"
              :size="32"
              class="text-muted-foreground mb-2 animate-spin"
            />
            <Terminal v-else :size="32" class="text-muted-foreground mb-2" />
            <span class="text-sm font-medium truncate w-full text-center">{{ session.name }}</span>
            <span class="text-xs text-muted-foreground mt-1">{{
              formatDate(
                session.status === 'stopped' && session.stopped_at
                  ? session.stopped_at
                  : session.created_at,
              )
            }}</span>
          </CardContent>
          <!-- Delete button (show on hover) -->
          <Button
            variant="ghost"
            size="icon"
            class="absolute top-1 right-1 h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-destructive"
            title="Delete terminal"
            :disabled="deletingIds.has(session.id)"
            @click="handleDeleteSession($event, session)"
          >
            <Loader2 v-if="deletingIds.has(session.id)" :size="14" class="animate-spin" />
            <Trash2 v-else :size="14" />
          </Button>
        </Card>

        <!-- Create new terminal card (uses Card with dashed border to match other terminal cards) -->
        <Card
          class="cursor-pointer border-dashed hover:border-primary transition-colors"
          :class="{ 'opacity-50': isCreating }"
          @click="handleCreateTerminal"
        >
          <CardContent
            class="flex flex-col items-center justify-center p-6 text-muted-foreground hover:text-foreground transition-colors"
          >
            <Loader2 v-if="isCreating" :size="32" class="mb-2 animate-spin" />
            <Plus v-else :size="32" class="mb-2" />
            <span class="text-sm">New Terminal</span>
          </CardContent>
        </Card>
      </div>

      <!-- List View -->
      <div v-else-if="viewMode === 'list'" class="space-y-1">
        <button
          v-for="session in filteredSessions"
          :key="session.id"
          class="group w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left hover:bg-muted"
          :class="{ 'opacity-50': deletingIds.has(session.id) || openingIds.has(session.id) }"
          @click="handleOpenSession(session)"
        >
          <!-- Status indicator -->
          <span
            v-if="session.status === 'running'"
            class="h-2 w-2 rounded-full bg-green-500 inline-block animate-pulse shrink-0"
            title="Running"
          />
          <span
            v-else
            class="h-2 w-2 rounded-full bg-gray-400 inline-block shrink-0"
            title="Stopped"
          />

          <!-- Icon -->
          <Loader2
            v-if="openingIds.has(session.id)"
            :size="20"
            class="text-muted-foreground shrink-0 animate-spin"
          />
          <Terminal v-else :size="20" class="text-muted-foreground shrink-0" />

          <span class="flex-1 text-sm truncate">{{ session.name }}</span>
          <span class="text-xs text-muted-foreground">{{
            formatDate(
              session.status === 'stopped' && session.stopped_at
                ? session.stopped_at
                : session.created_at,
            )
          }}</span>

          <!-- Delete button (show on hover) -->
          <Button
            variant="ghost"
            size="icon"
            class="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-destructive shrink-0"
            title="Delete terminal"
            :disabled="deletingIds.has(session.id)"
            @click="handleDeleteSession($event, session)"
          >
            <Loader2 v-if="deletingIds.has(session.id)" :size="14" class="animate-spin" />
            <Trash2 v-else :size="14" />
          </Button>
        </button>

        <!-- Create new terminal row (uses dashed border to match card style in grid view) -->
        <button
          class="w-full flex items-center gap-3 px-3 py-2 rounded-lg transition-all text-left border-2 border-dashed hover:border-primary hover:bg-muted/50"
          :class="{ 'opacity-50': isCreating }"
          @click="handleCreateTerminal"
        >
          <Plus :size="20" class="text-muted-foreground shrink-0" />
          <span class="flex-1 text-sm text-muted-foreground">New Terminal</span>
        </button>
      </div>
    </div>
  </div>
</template>
