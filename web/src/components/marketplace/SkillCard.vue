<script setup lang="ts">
import { computed } from 'vue'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Download, Star, ExternalLink, Github, Package } from 'lucide-vue-next'
import type { MarketplaceSearchResult } from '@/api/marketplace'

const props = defineProps<{
  skill: MarketplaceSearchResult
  installed?: boolean
  installing?: boolean
}>()

const emit = defineEmits<{
  (e: 'install', id: string): void
  (e: 'uninstall', id: string): void
  (e: 'view-details', id: string): void
}>()

const sourceIcon = computed(() => {
  switch (props.skill.source) {
    case 'github':
      return Github
    case 'marketplace':
    default:
      return Package
  }
})

const sourceBadgeVariant = computed(() => {
  switch (props.skill.source) {
    case 'github':
      return 'secondary'
    case 'marketplace':
      return 'default'
    default:
      return 'outline'
  }
})

function formatDownloads(count?: number): string {
  if (!count) return '0'
  if (count >= 1000000) return `${(count / 1000000).toFixed(1)}M`
  if (count >= 1000) return `${(count / 1000).toFixed(1)}K`
  return count.toString()
}

function handleInstall() {
  emit('install', props.skill.manifest.id)
}

function handleUninstall() {
  emit('uninstall', props.skill.manifest.id)
}

function handleViewDetails() {
  emit('view-details', props.skill.manifest.id)
}

function openRepository() {
  if (props.skill.manifest.repository) {
    window.open(props.skill.manifest.repository, '_blank')
  }
}
</script>

<template>
  <Card
    class="flex flex-col h-full hover:shadow-lg transition-shadow cursor-pointer"
    @click="handleViewDetails"
  >
    <CardHeader class="pb-2">
      <div class="flex items-start justify-between">
        <div class="flex-1 min-w-0">
          <CardTitle class="text-lg truncate">{{ skill.manifest.name }}</CardTitle>
          <CardDescription class="text-sm text-muted-foreground">
            {{ skill.manifest.author?.name || 'Unknown author' }}
          </CardDescription>
        </div>
        <Badge :variant="sourceBadgeVariant" class="ml-2 shrink-0">
          <component :is="sourceIcon" class="w-3 h-3 mr-1" />
          {{ skill.source }}
        </Badge>
      </div>
    </CardHeader>

    <CardContent class="flex-1 pt-0">
      <p class="text-sm text-muted-foreground line-clamp-2 mb-3">
        {{ skill.manifest.description || 'No description available' }}
      </p>

      <div class="flex flex-wrap gap-1 mb-3">
        <Badge
          v-for="keyword in skill.manifest.keywords?.slice(0, 3)"
          :key="keyword"
          variant="outline"
          class="text-xs"
        >
          {{ keyword }}
        </Badge>
        <Badge v-if="(skill.manifest.keywords?.length || 0) > 3" variant="outline" class="text-xs">
          +{{ skill.manifest.keywords!.length - 3 }}
        </Badge>
      </div>

      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        <div class="flex items-center gap-1" v-if="skill.downloads != null">
          <Download class="w-3 h-3" />
          {{ formatDownloads(skill.downloads) }}
        </div>
        <div class="flex items-center gap-1" v-if="skill.rating != null">
          <Star class="w-3 h-3 fill-yellow-400 text-yellow-400" />
          {{ skill.rating.toFixed(1) }}
        </div>
        <div class="flex items-center gap-1">
          v{{ skill.manifest.version?.major || 0 }}.{{ skill.manifest.version?.minor || 0 }}.{{
            skill.manifest.version?.patch || 0
          }}
        </div>
      </div>
    </CardContent>

    <CardFooter class="pt-0">
      <div class="flex gap-2 w-full" @click.stop>
        <Button
          v-if="installed"
          variant="outline"
          size="sm"
          class="flex-1"
          @click="handleUninstall"
        >
          Uninstall
        </Button>
        <Button v-else size="sm" class="flex-1" :disabled="installing" @click="handleInstall">
          <Download class="w-4 h-4 mr-1" />
          {{ installing ? 'Installing...' : 'Install' }}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          v-if="skill.manifest.repository"
          @click.stop="openRepository"
        >
          <ExternalLink class="w-4 h-4" />
        </Button>
      </div>
    </CardFooter>
  </Card>
</template>
