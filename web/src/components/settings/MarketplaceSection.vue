<script setup lang="ts">
import { onMounted, ref } from 'vue'
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
  getMarketplaceSkillDetail,
  installMarketplaceSkill,
  listInstalledMarketplaceSkills,
  listMarketplaceCategories,
  searchMarketplace,
  updateMarketplaceSkill,
  uninstallMarketplaceSkill,
  type MarketplaceCategory,
  type MarketplaceSearchItem,
  type MarketplaceSkillDetail,
  type MarketplaceSource,
} from '@/api/marketplace'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import type { Skill, SkillVersion } from '@/types/generated'

type MarketplaceSort = 'popular' | 'relevance' | 'recently_updated' | 'name'

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()

const query = ref('')
const includeGithub = ref(false)
const selectedCategory = ref('all')
const selectedSort = ref<MarketplaceSort>('popular')

const loading = ref(false)
const syncingInstalled = ref(false)
const loadingCategories = ref(false)
const loadingDetail = ref(false)
const actionInProgressId = ref<string | null>(null)
const error = ref<string | null>(null)

const searchResults = ref<MarketplaceSearchItem[]>([])
const installedSkills = ref<Skill[]>([])
const categories = ref<MarketplaceCategory[]>([])

const showDetailDialog = ref(false)
const detail = ref<MarketplaceSkillDetail | null>(null)
const showInstallDialog = ref(false)
const installTarget = ref<MarketplaceSearchItem | null>(null)
const installVersionOptions = ref<string[]>([])
const selectedInstallVersion = ref('__latest__')

const latestVersionValue = '__latest__'

const sortOptions: Array<{ value: MarketplaceSort; label: string }> = [
  { value: 'popular', label: 'settings.marketplace.sortPopular' },
  { value: 'relevance', label: 'settings.marketplace.sortRelevance' },
  { value: 'recently_updated', label: 'settings.marketplace.sortUpdated' },
  { value: 'name', label: 'settings.marketplace.sortName' },
]

function resolveSource(source: string): MarketplaceSource {
  return source === 'github' ? 'github' : 'marketplace'
}

function handleIncludeGithubChange(value: boolean) {
  includeGithub.value = value
  void runSearch()
}

function formatVersion(version: SkillVersion): string {
  const base = `${version.major}.${version.minor}.${version.patch}`
  return version.prerelease ? `${base}-${version.prerelease}` : base
}

function isInstalled(id: string): boolean {
  return installedSkills.value.some((skill) => skill.id === id)
}

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

async function loadCategories() {
  loadingCategories.value = true
  try {
    categories.value = await listMarketplaceCategories()
  } catch {
    categories.value = []
  } finally {
    loadingCategories.value = false
  }
}

async function runSearch() {
  loading.value = true
  error.value = null
  try {
    searchResults.value = await searchMarketplace({
      query: query.value.trim() || undefined,
      category: selectedCategory.value === 'all' ? undefined : selectedCategory.value,
      include_github: includeGithub.value,
      limit: 50,
      offset: 0,
      sort: selectedSort.value,
    })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function installSkill(item: MarketplaceSearchItem) {
  error.value = null
  actionInProgressId.value = item.manifest.id
  try {
    const targetSource = resolveSource(item.source)
    const skillDetail = await getMarketplaceSkillDetail(item.manifest.id, targetSource)
    const versions = skillDetail.versions.map((version) => formatVersion(version))
    installVersionOptions.value = versions
    selectedInstallVersion.value = versions[0] ?? latestVersionValue
    installTarget.value = item
    showInstallDialog.value = true
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    toast.error(t('settings.marketplace.loadDetailsFailed'))
  } finally {
    actionInProgressId.value = null
  }
}

function closeInstallDialog() {
  showInstallDialog.value = false
  installTarget.value = null
  installVersionOptions.value = []
  selectedInstallVersion.value = latestVersionValue
}

async function confirmInstall() {
  if (!installTarget.value) return

  const item = installTarget.value
  error.value = null
  actionInProgressId.value = item.manifest.id
  try {
    const result = await installMarketplaceSkill({
      id: item.manifest.id,
      source: resolveSource(item.source),
      version: selectedInstallVersion.value === latestVersionValue ? undefined : selectedInstallVersion.value,
      overwrite: false,
    })

    if (!result.success) {
      error.value = result.error ?? 'Install failed.'
      return
    }

    toast.success(t('settings.marketplace.installSuccess'))
    await loadInstalled()
    closeInstallDialog()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    toast.error(t('settings.marketplace.installFailed'))
  } finally {
    actionInProgressId.value = null
  }
}

async function updateSkill(skill: Skill) {
  error.value = null
  actionInProgressId.value = skill.id
  try {
    const result = await updateMarketplaceSkill(skill.id, 'marketplace')
    if (!result.success) {
      error.value = result.error ?? 'Update failed.'
      return
    }

    toast.success(t('settings.marketplace.updateSuccess'))
    await loadInstalled()
    await runSearch()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    toast.error(t('settings.marketplace.updateFailed'))
  } finally {
    actionInProgressId.value = null
  }
}

async function uninstallSkill(id: string, name: string) {
  const confirmed = await confirm({
    title: t('settings.marketplace.uninstallConfirmTitle'),
    description: t('settings.marketplace.uninstallConfirmDescription', { name }),
    confirmText: t('settings.marketplace.uninstall'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })

  if (!confirmed) return

  error.value = null
  actionInProgressId.value = id
  const result = await uninstallMarketplaceSkill(id)
  if (!result.success) {
    error.value = result.error ?? 'Uninstall failed.'
    actionInProgressId.value = null
    return
  }

  toast.success(t('settings.marketplace.uninstallSuccess'))
  await loadInstalled()
  await runSearch()
  actionInProgressId.value = null
}

async function openDetail(id: string, source: MarketplaceSource) {
  loadingDetail.value = true
  detail.value = null
  showDetailDialog.value = true
  try {
    detail.value = await getMarketplaceSkillDetail(id, source)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    toast.error(t('settings.marketplace.loadDetailsFailed'))
  } finally {
    loadingDetail.value = false
  }
}

onMounted(async () => {
  await Promise.all([loadInstalled(), loadCategories(), runSearch()])
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">{{ t('settings.marketplace.title') }}</h2>
        <p class="text-muted-foreground">{{ t('settings.marketplace.description') }}</p>
      </div>
      <Button variant="outline" :disabled="syncingInstalled" @click="loadInstalled">
        {{ t('settings.marketplace.refreshInstalled') }}
      </Button>
    </div>

    <div class="rounded-lg border p-4 space-y-3">
      <div class="flex items-center gap-2">
        <Input
          v-model="query"
          :placeholder="t('settings.marketplace.searchPlaceholder')"
          @keydown.enter.prevent="runSearch"
        />
        <Button :disabled="loading" @click="runSearch">{{ t('settings.marketplace.search') }}</Button>
      </div>

      <div class="grid gap-3 md:grid-cols-3">
        <div class="space-y-2">
          <Label>{{ t('settings.marketplace.categoryLabel') }}</Label>
          <Select
            v-model="selectedCategory"
            :disabled="loadingCategories"
            @update:model-value="runSearch"
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{{ t('settings.marketplace.allCategories') }}</SelectItem>
              <SelectItem v-for="category in categories" :key="category.name" :value="category.name">
                {{ category.name }} ({{ category.count }})
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="space-y-2">
          <Label>{{ t('settings.marketplace.sortLabel') }}</Label>
          <Select v-model="selectedSort" @update:model-value="runSearch">
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="option in sortOptions" :key="option.value" :value="option.value">
                {{ t(option.label) }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="flex items-end">
          <div class="flex w-full items-center justify-between rounded-md border bg-background px-3 py-2">
            <Label for="include-github">{{ t('settings.marketplace.includeGithub') }}</Label>
            <Switch
              id="include-github"
              :checked="includeGithub"
              @update:checked="(value) => handleIncludeGithubChange(Boolean(value))"
            />
          </div>
        </div>
      </div>
    </div>

    <div v-if="error" class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {{ error }}
    </div>

    <div class="grid gap-4 lg:grid-cols-2">
      <section class="space-y-3">
        <h3 class="text-base font-semibold">{{ t('settings.marketplace.searchResults') }}</h3>
        <div v-if="loading" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.marketplace.searching') }}
        </div>
        <div v-else-if="searchResults.length === 0" class="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
          {{ t('settings.marketplace.noResults') }}
        </div>
        <div v-else class="space-y-3">
          <div
            v-for="result in searchResults"
            :key="result.manifest.id"
            class="rounded-lg border bg-card p-4"
          >
            <div class="space-y-3">
              <div class="flex items-start justify-between gap-3">
                <div class="space-y-1">
                  <div class="flex items-center gap-2">
                    <h4 class="font-medium">{{ result.manifest.name }}</h4>
                    <Badge variant="outline">{{ result.source }}</Badge>
                  </div>
                  <p class="text-sm text-muted-foreground">
                    {{ result.manifest.description || t('settings.marketplace.noDescription') }}
                  </p>
                  <p class="text-xs text-muted-foreground">
                    id: {{ result.manifest.id }} ·
                    {{ t('settings.marketplace.version') }}: {{ formatVersion(result.manifest.version) }} ·
                    downloads: {{ result.downloads ?? 0 }} · score: {{ result.score }}
                  </p>
                </div>
              </div>

              <div class="flex items-center justify-end gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  :disabled="actionInProgressId === result.manifest.id"
                  @click="openDetail(result.manifest.id, resolveSource(result.source))"
                >
                  {{ t('settings.marketplace.details') }}
                </Button>
                <Button
                  v-if="!isInstalled(result.manifest.id)"
                  size="sm"
                  :disabled="actionInProgressId === result.manifest.id"
                  @click="installSkill(result)"
                >
                  {{ t('settings.marketplace.install') }}
                </Button>
                <Button
                  v-else
                  size="sm"
                  variant="outline"
                  :disabled="actionInProgressId === result.manifest.id"
                  @click="uninstallSkill(result.manifest.id, result.manifest.name)"
                >
                  {{ t('settings.marketplace.uninstall') }}
                </Button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section class="space-y-3">
        <h3 class="text-base font-semibold">{{ t('settings.marketplace.installedSkills') }}</h3>
        <div v-if="syncingInstalled" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.marketplace.loadingInstalled') }}
        </div>
        <div
          v-else-if="installedSkills.length === 0"
          class="rounded-md border border-dashed p-4 text-sm text-muted-foreground"
        >
          {{ t('settings.marketplace.noInstalled') }}
        </div>
        <div v-else class="space-y-3">
          <div
            v-for="skill in installedSkills"
            :key="skill.id"
            class="rounded-lg border bg-card p-4"
          >
            <div class="space-y-3">
              <div>
                <h4 class="font-medium">{{ skill.name }}</h4>
                <p class="text-sm text-muted-foreground">
                  {{ skill.description || t('settings.marketplace.noDescription') }}
                </p>
                <p class="text-xs text-muted-foreground">
                  id: {{ skill.id }} ·
                  {{ t('settings.marketplace.version') }}: {{ skill.version || '-' }}
                </p>
              </div>
              <div class="flex items-center justify-end gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  :disabled="actionInProgressId === skill.id"
                  @click="openDetail(skill.id, 'marketplace')"
                >
                  {{ t('settings.marketplace.details') }}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  :disabled="actionInProgressId === skill.id"
                  @click="updateSkill(skill)"
                >
                  {{ t('settings.marketplace.update') }}
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  :disabled="actionInProgressId === skill.id"
                  @click="uninstallSkill(skill.id, skill.name)"
                >
                  {{ t('settings.marketplace.uninstall') }}
                </Button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <Dialog v-model:open="showInstallDialog">
      <DialogContent class="max-w-[32rem]">
        <DialogHeader>
          <DialogTitle>{{ t('settings.marketplace.install') }}</DialogTitle>
          <DialogDescription>
            {{ installTarget?.manifest.name || '' }}
          </DialogDescription>
        </DialogHeader>

        <div class="space-y-2">
          <Label>{{ t('settings.marketplace.version') }}</Label>
          <Select v-model="selectedInstallVersion">
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem :value="latestVersionValue">{{ t('settings.marketplace.latestVersion') }}</SelectItem>
              <SelectItem v-for="version in installVersionOptions" :key="version" :value="version">
                {{ version }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <DialogFooter>
          <Button variant="outline" @click="closeInstallDialog">
            {{ t('common.cancel') }}
          </Button>
          <Button
            :disabled="!installTarget || actionInProgressId === installTarget.manifest.id"
            @click="confirmInstall"
          >
            {{ t('settings.marketplace.install') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <Dialog v-model:open="showDetailDialog">
      <DialogContent class="max-w-[48rem]">
        <DialogHeader>
          <DialogTitle>
            {{ detail?.manifest.name || t('settings.marketplace.details') }}
          </DialogTitle>
          <DialogDescription>
            {{ detail?.manifest.id || '' }}
          </DialogDescription>
        </DialogHeader>

        <div v-if="loadingDetail" class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          {{ t('settings.marketplace.details') }}
        </div>

        <div v-else-if="detail" class="space-y-4">
          <div class="grid gap-3 sm:grid-cols-2">
            <div class="rounded-md border bg-muted/30 px-3 py-2 text-sm">
              <p class="text-muted-foreground">{{ t('settings.marketplace.version') }}</p>
              <p class="font-medium">{{ formatVersion(detail.manifest.version) }}</p>
            </div>
            <div class="rounded-md border bg-muted/30 px-3 py-2 text-sm">
              <p class="text-muted-foreground">{{ t('settings.marketplace.author') }}</p>
              <p class="font-medium">{{ detail.manifest.author?.name || '-' }}</p>
            </div>
            <div class="rounded-md border bg-muted/30 px-3 py-2 text-sm">
              <p class="text-muted-foreground">{{ t('settings.marketplace.license') }}</p>
              <p class="font-medium">{{ detail.manifest.license || '-' }}</p>
            </div>
            <div class="rounded-md border bg-muted/30 px-3 py-2 text-sm">
              <p class="text-muted-foreground">{{ t('settings.marketplace.repository') }}</p>
              <p class="truncate font-medium">{{ detail.manifest.repository || '-' }}</p>
            </div>
          </div>

          <div class="rounded-md border bg-muted/30 p-3 space-y-2">
            <div class="flex items-center justify-between">
              <h4 class="text-sm font-medium">{{ t('settings.marketplace.gating') }}</h4>
              <Badge :variant="detail.gating.passed ? 'default' : 'destructive'">
                {{ detail.gating.passed ? t('settings.marketplace.gatingPassed') : t('settings.marketplace.gatingFailed') }}
              </Badge>
            </div>
            <p class="text-sm text-muted-foreground">{{ detail.gating.summary }}</p>
            <div
              v-if="detail.gating.missing_binaries.length > 0"
              class="rounded border border-destructive/30 bg-destructive/5 px-2 py-1 text-xs text-destructive"
            >
              {{ t('settings.marketplace.requiredBinaries') }}: {{ detail.gating.missing_binaries.join(', ') }}
            </div>
            <div
              v-if="detail.gating.missing_env_vars.length > 0"
              class="rounded border border-destructive/30 bg-destructive/5 px-2 py-1 text-xs text-destructive"
            >
              {{ t('settings.marketplace.requiredEnvVars') }}: {{ detail.gating.missing_env_vars.join(', ') }}
            </div>
          </div>

          <div class="space-y-2">
            <h4 class="text-sm font-medium">{{ t('settings.marketplace.contentPreview') }}</h4>
            <div
              v-if="detail.content"
              class="max-h-64 overflow-auto rounded-md border bg-muted/30 p-3 font-mono text-xs whitespace-pre-wrap"
            >
              {{ detail.content }}
            </div>
            <div v-else class="rounded-md border border-dashed p-3 text-sm text-muted-foreground">
              {{ t('settings.marketplace.noContent') }}
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" @click="showDetailDialog = false">
            {{ t('settings.marketplace.close') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
