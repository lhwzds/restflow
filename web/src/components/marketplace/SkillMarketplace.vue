<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Search, RefreshCw, Filter, X } from 'lucide-vue-next'
import SkillCard from './SkillCard.vue'
import SkillDetailModal from './SkillDetailModal.vue'
import type { MarketplaceSearchResult } from '@/api/marketplace'
import { searchMarketplace, listInstalledSkills, uninstallSkill } from '@/api/marketplace'
import { useToast } from '@/components/ui/toast/use-toast'
import { useDebounceFn } from '@vueuse/core'

const { toast } = useToast()

// State
const searchQuery = ref('')
const sortOrder = ref<'relevance' | 'updated' | 'popular' | 'name'>('relevance')
const includeGithub = ref(true)
const selectedCategory = ref<string>('')
const loading = ref(false)
const results = ref<MarketplaceSearchResult[]>([])
const installedSkillIds = ref<Set<string>>(new Set())
const installingIds = ref<Set<string>>(new Set())

// Modal state
const showDetailModal = ref(false)
const selectedSkill = ref<MarketplaceSearchResult | null>(null)

// Categories
const categories = [
  'All',
  'AI & LLM',
  'Development',
  'Productivity',
  'Communication',
  'Data',
  'Automation',
  'Security',
  'Other',
]

// Debounced search
const debouncedSearch = useDebounceFn(async () => {
  await performSearch()
}, 300)

// Watch search inputs
watch([searchQuery], () => {
  debouncedSearch()
})

watch([sortOrder, includeGithub, selectedCategory], () => {
  performSearch()
})

async function performSearch() {
  loading.value = true
  try {
    results.value = await searchMarketplace({
      query: searchQuery.value || undefined,
      category: selectedCategory.value && selectedCategory.value !== 'All' ? selectedCategory.value : undefined,
      sort: sortOrder.value,
      includeGithub: includeGithub.value,
      limit: 50,
    })
  } catch (error) {
    console.error('Search failed:', error)
    toast({
      title: 'Search failed',
      description: error instanceof Error ? error.message : 'Unknown error occurred',
      variant: 'destructive',
    })
  } finally {
    loading.value = false
  }
}

async function loadInstalledSkills() {
  try {
    const installed = await listInstalledSkills()
    installedSkillIds.value = new Set(installed.map((s: any) => s.id))
  } catch (error) {
    console.error('Failed to load installed skills:', error)
  }
}

function handleViewDetails(id: string) {
  const skill = results.value.find(r => r.manifest.id === id)
  if (skill) {
    selectedSkill.value = skill
    showDetailModal.value = true
  }
}

async function handleInstall(id: string) {
  installingIds.value.add(id)
  // Install will be handled by the detail modal
  handleViewDetails(id)
  installingIds.value.delete(id)
}

async function handleUninstall(id: string) {
  try {
    await uninstallSkill(id)
    installedSkillIds.value.delete(id)
    toast({
      title: 'Skill uninstalled',
      description: 'The skill has been removed.',
    })
  } catch (error) {
    toast({
      title: 'Uninstall failed',
      description: error instanceof Error ? error.message : 'Unknown error occurred',
      variant: 'destructive',
    })
  }
}

function handleSkillInstalled(id: string) {
  installedSkillIds.value.add(id)
  loadInstalledSkills()
}

function clearFilters() {
  searchQuery.value = ''
  selectedCategory.value = ''
  sortOrder.value = 'relevance'
}

onMounted(async () => {
  await Promise.all([
    performSearch(),
    loadInstalledSkills(),
  ])
})
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <div class="border-b p-4 space-y-4">
      <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold">Skill Marketplace</h1>
        <Button variant="outline" size="sm" @click="performSearch">
          <RefreshCw class="w-4 h-4 mr-1" :class="{ 'animate-spin': loading }" />
          Refresh
        </Button>
      </div>

      <!-- Search & Filters -->
      <div class="flex flex-col sm:flex-row gap-4">
        <div class="relative flex-1">
          <Search class="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            v-model="searchQuery"
            placeholder="Search skills..."
            class="pl-9"
          />
        </div>
        
        <div class="flex gap-2">
          <Select v-model="selectedCategory">
            <SelectTrigger class="w-[150px]">
              <SelectValue placeholder="Category" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="cat in categories" :key="cat" :value="cat">
                {{ cat }}
              </SelectItem>
            </SelectContent>
          </Select>

          <Select v-model="sortOrder">
            <SelectTrigger class="w-[150px]">
              <SelectValue placeholder="Sort by" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="relevance">Relevance</SelectItem>
              <SelectItem value="popular">Popular</SelectItem>
              <SelectItem value="updated">Recently Updated</SelectItem>
              <SelectItem value="name">Name</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      <!-- Active Filters -->
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2">
          <Switch id="github" v-model:checked="includeGithub" />
          <Label for="github" class="text-sm">Include GitHub</Label>
        </div>

        <div v-if="searchQuery || (selectedCategory && selectedCategory !== 'All')" class="flex items-center gap-2">
          <Badge v-if="searchQuery" variant="secondary" class="gap-1">
            "{{ searchQuery }}"
            <X class="w-3 h-3 cursor-pointer" @click="searchQuery = ''" />
          </Badge>
          <Badge v-if="selectedCategory && selectedCategory !== 'All'" variant="secondary" class="gap-1">
            {{ selectedCategory }}
            <X class="w-3 h-3 cursor-pointer" @click="selectedCategory = ''" />
          </Badge>
          <Button variant="ghost" size="sm" @click="clearFilters">
            Clear all
          </Button>
        </div>
      </div>
    </div>

    <!-- Results -->
    <ScrollArea class="flex-1">
      <div class="p-4">
        <!-- Loading State -->
        <div v-if="loading" class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          <div v-for="i in 8" :key="i" class="space-y-3">
            <Skeleton class="h-[200px] w-full rounded-lg" />
          </div>
        </div>

        <!-- Empty State -->
        <div v-else-if="results.length === 0" class="flex flex-col items-center justify-center py-16">
          <Filter class="w-12 h-12 text-muted-foreground mb-4" />
          <h3 class="text-lg font-medium mb-2">No skills found</h3>
          <p class="text-muted-foreground text-center max-w-md">
            Try adjusting your search or filters to find what you're looking for.
          </p>
          <Button variant="outline" class="mt-4" @click="clearFilters">
            Clear filters
          </Button>
        </div>

        <!-- Results Grid -->
        <div v-else class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          <SkillCard
            v-for="skill in results"
            :key="skill.manifest.id"
            :skill="skill"
            :installed="installedSkillIds.has(skill.manifest.id)"
            :installing="installingIds.has(skill.manifest.id)"
            @install="handleInstall"
            @uninstall="handleUninstall"
            @view-details="handleViewDetails"
          />
        </div>
      </div>
    </ScrollArea>

    <!-- Detail Modal -->
    <SkillDetailModal
      v-model:open="showDetailModal"
      :skill="selectedSkill?.manifest ?? null"
      :source="selectedSkill?.source"
      :installed="selectedSkill ? installedSkillIds.has(selectedSkill.manifest.id) : false"
      @installed="handleSkillInstalled"
    />
  </div>
</template>
