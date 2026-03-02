<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  installMarketplaceSkill,
  listInstalledMarketplaceSkills,
  searchMarketplace,
  uninstallMarketplaceSkill,
  type MarketplaceSearchItem,
} from '@/api/marketplace'
import type { Skill } from '@/types/generated'

const query = ref('')
const includeGithub = ref(false)
const loading = ref(false)
const syncingInstalled = ref(false)
const error = ref<string | null>(null)

const searchResults = ref<MarketplaceSearchItem[]>([])
const installedSkills = ref<Skill[]>([])

async function loadInstalled() {
  syncingInstalled.value = true
  try {
    installedSkills.value = await listInstalledMarketplaceSkills()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    syncingInstalled.value = false
  }
}

async function runSearch() {
  loading.value = true
  error.value = null
  try {
    searchResults.value = await searchMarketplace({
      query: query.value.trim() || undefined,
      include_github: includeGithub.value,
      limit: 50,
      offset: 0,
      sort: 'popular',
    })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

function isInstalled(id: string): boolean {
  return installedSkills.value.some((skill) => skill.id === id)
}

async function installSkill(id: string, source: 'marketplace' | 'github' = 'marketplace') {
  error.value = null
  const result = await installMarketplaceSkill({ id, source, overwrite: false })
  if (!result.success) {
    error.value = result.error ?? 'Install failed.'
    return
  }
  await loadInstalled()
}

async function uninstallSkill(id: string) {
  error.value = null
  const result = await uninstallMarketplaceSkill(id)
  if (!result.success) {
    error.value = result.error ?? 'Uninstall failed.'
    return
  }
  await loadInstalled()
}

onMounted(async () => {
  await Promise.all([loadInstalled(), runSearch()])
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">Marketplace</h2>
        <p class="text-muted-foreground">Browse and install skills that are available from backend marketplace providers.</p>
      </div>
      <Button variant="outline" :disabled="syncingInstalled" @click="loadInstalled">Refresh Installed</Button>
    </div>

    <div class="rounded-lg border p-4 space-y-3">
      <div class="flex items-center gap-2">
        <Input
          v-model="query"
          placeholder="Search skills by name, tag, or author"
          @keydown.enter.prevent="runSearch"
        />
        <Button :disabled="loading" @click="runSearch">Search</Button>
      </div>
      <label class="flex items-center gap-2 text-sm text-muted-foreground">
        <input
          v-model="includeGithub"
          type="checkbox"
          class="h-4 w-4"
          @change="runSearch"
        />
        Include GitHub source
      </label>
    </div>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="space-y-3">
        <h3 class="text-base font-semibold">Search Results</h3>
        <div v-if="loading" class="text-sm text-muted-foreground">Searching...</div>
        <div v-else-if="searchResults.length === 0" class="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
          No skills found.
        </div>
        <div
          v-for="result in searchResults"
          v-else
          :key="result.manifest.id"
          class="rounded-lg border bg-card p-4"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="space-y-1">
              <div class="flex items-center gap-2">
                <h4 class="font-medium">{{ result.manifest.name }}</h4>
                <Badge variant="outline">{{ result.source }}</Badge>
              </div>
              <p class="text-sm text-muted-foreground">{{ result.manifest.description || 'No description' }}</p>
              <p class="text-xs text-muted-foreground">
                id: {{ result.manifest.id }} · downloads: {{ result.downloads ?? 0 }} · score: {{ result.score }}
              </p>
            </div>

            <Button
              v-if="!isInstalled(result.manifest.id)"
              size="sm"
              @click="installSkill(result.manifest.id, result.source === 'github' ? 'github' : 'marketplace')"
            >
              Install
            </Button>
            <Button
              v-else
              size="sm"
              variant="outline"
              @click="uninstallSkill(result.manifest.id)"
            >
              Uninstall
            </Button>
          </div>
        </div>
      </section>

      <section class="space-y-3">
        <h3 class="text-base font-semibold">Installed Skills</h3>
        <div v-if="syncingInstalled" class="text-sm text-muted-foreground">Loading installed skills...</div>
        <div
          v-else-if="installedSkills.length === 0"
          class="rounded-md border border-dashed p-4 text-sm text-muted-foreground"
        >
          No installed skills found.
        </div>
        <div
          v-for="skill in installedSkills"
          v-else
          :key="skill.id"
          class="rounded-lg border bg-card p-4"
        >
          <div class="flex items-start justify-between gap-3">
            <div>
              <h4 class="font-medium">{{ skill.name }}</h4>
              <p class="text-sm text-muted-foreground">{{ skill.description || 'No description' }}</p>
              <p class="text-xs text-muted-foreground">id: {{ skill.id }}</p>
            </div>
            <Button size="sm" variant="destructive" @click="uninstallSkill(skill.id)">Uninstall</Button>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
