<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Skeleton } from '@/components/ui/skeleton'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Download, ExternalLink, Github, Package, AlertTriangle, Check, X } from 'lucide-vue-next'
import type { SkillManifest, SkillVersion, GatingCheckResult } from '@/types/generated'
import {
  getSkillContent,
  getSkillVersions,
  checkSkillGating,
  installSkill,
} from '@/api/marketplace'
import { useToast } from '@/components/ui/toast/use-toast'
import { formatSkillVersion } from '@/utils/skillVersion'

const props = defineProps<{
  open: boolean
  skill: SkillManifest | null
  source?: string
  installed?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (e: 'installed', id: string): void
}>()

const { toast } = useToast()

const loading = ref(false)
const installing = ref(false)
const content = ref('')
const versions = ref<SkillVersion[]>([])
const gatingResult = ref<GatingCheckResult | null>(null)
const selectedVersion = ref<string>('')
const activeTab = ref('readme')

const versionKey = (version: SkillVersion) => formatSkillVersion(version)

const versionString = computed(() => {
  if (!props.skill?.version) return '0.0.0'
  return formatSkillVersion(props.skill.version)
})

watch(
  () => props.open,
  async (isOpen) => {
    if (isOpen && props.skill) {
      loading.value = true
      try {
        // Fetch content, versions, and gating in parallel
        const [contentResult, versionsResult, gatingResultData] = await Promise.allSettled([
          getSkillContent(props.skill.id, undefined, props.source),
          getSkillVersions(props.skill.id, props.source),
          checkSkillGating(props.skill.id, props.source),
        ])

        if (contentResult.status === 'fulfilled') {
          content.value = contentResult.value
        }
        if (versionsResult.status === 'fulfilled') {
          versions.value = versionsResult.value
          if (versions.value.length > 0 && !selectedVersion.value) {
            const v = versions.value[0]
            if (v) {
              selectedVersion.value = versionKey(v)
            }
          }
        }
        if (gatingResultData.status === 'fulfilled') {
          gatingResult.value = gatingResultData.value
        }
      } catch (error) {
        console.error('Failed to load skill details:', error)
      } finally {
        loading.value = false
      }
    }
  },
)

function handleClose() {
  emit('update:open', false)
}

async function handleInstall() {
  if (!props.skill) return

  installing.value = true
  try {
    await installSkill(props.skill.id, selectedVersion.value || undefined, props.source)
    toast({
      title: 'Skill installed',
      description: `${props.skill.name} has been installed successfully.`,
    })
    emit('installed', props.skill.id)
    handleClose()
  } catch (error) {
    toast({
      title: 'Installation failed',
      description: error instanceof Error ? error.message : 'Unknown error occurred',
      variant: 'destructive',
    })
  } finally {
    installing.value = false
  }
}

function openRepository() {
  if (props.skill?.repository) {
    window.open(props.skill.repository, '_blank')
  }
}

function openHomepage() {
  if (props.skill?.homepage) {
    window.open(props.skill.homepage, '_blank')
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="max-w-[48rem] max-h-[90vh] flex flex-col">
      <DialogHeader>
        <div class="flex items-start justify-between">
          <div>
            <DialogTitle class="text-xl">{{ skill?.name }}</DialogTitle>
            <DialogDescription class="flex items-center gap-2 mt-1">
              <span>{{ skill?.author?.name || 'Unknown author' }}</span>
              <span class="text-muted-foreground">•</span>
              <span>v{{ versionString }}</span>
            </DialogDescription>
          </div>
          <Badge variant="outline">
            <component :is="source === 'github' ? Github : Package" class="w-3 h-3 mr-1" />
            {{ source || 'marketplace' }}
          </Badge>
        </div>
      </DialogHeader>

      <Tabs v-model="activeTab" class="flex-1 flex flex-col min-h-0">
        <TabsList class="grid w-full grid-cols-3">
          <TabsTrigger value="readme">Readme</TabsTrigger>
          <TabsTrigger value="requirements">Requirements</TabsTrigger>
          <TabsTrigger value="versions">Versions</TabsTrigger>
        </TabsList>

        <TabsContent value="readme" class="flex-1 mt-4 min-h-0">
          <ScrollArea class="h-[400px] rounded-md border p-4">
            <div v-if="loading" class="space-y-2">
              <Skeleton class="h-4 w-full" />
              <Skeleton class="h-4 w-3/4" />
              <Skeleton class="h-4 w-5/6" />
            </div>
            <div v-else-if="content" class="prose prose-sm dark:prose-invert max-w-none">
              <pre class="whitespace-pre-wrap font-sans text-sm">{{ content }}</pre>
            </div>
            <p v-else class="text-muted-foreground text-center py-8">No documentation available</p>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="requirements" class="flex-1 mt-4 min-h-0">
          <ScrollArea class="h-[400px] rounded-md border p-4">
            <div v-if="loading" class="space-y-2">
              <Skeleton class="h-16 w-full" />
            </div>
            <div v-else-if="gatingResult" class="space-y-4">
              <Alert :variant="gatingResult.passed ? 'default' : 'destructive'">
                <component :is="gatingResult.passed ? Check : AlertTriangle" class="h-4 w-4" />
                <AlertTitle>{{
                  gatingResult.passed ? 'Ready to install' : 'Requirements not met'
                }}</AlertTitle>
                <AlertDescription>{{ gatingResult.summary }}</AlertDescription>
              </Alert>

              <div v-if="gatingResult.missing_binaries?.length" class="space-y-2">
                <h4 class="font-medium text-sm">Missing Binaries</h4>
                <div class="flex flex-wrap gap-2">
                  <Badge
                    v-for="bin in gatingResult.missing_binaries"
                    :key="bin"
                    variant="destructive"
                  >
                    <X class="w-3 h-3 mr-1" />
                    {{ bin }}
                  </Badge>
                </div>
              </div>

              <div v-if="gatingResult.missing_env_vars?.length" class="space-y-2">
                <h4 class="font-medium text-sm">Missing Environment Variables</h4>
                <div class="flex flex-wrap gap-2">
                  <Badge
                    v-for="env in gatingResult.missing_env_vars"
                    :key="env"
                    variant="destructive"
                  >
                    <X class="w-3 h-3 mr-1" />
                    {{ env }}
                  </Badge>
                </div>
              </div>

              <div v-if="!gatingResult.os_supported" class="space-y-2">
                <Alert variant="destructive">
                  <AlertTriangle class="h-4 w-4" />
                  <AlertTitle>OS Not Supported</AlertTitle>
                  <AlertDescription>
                    This skill is not compatible with your operating system.
                  </AlertDescription>
                </Alert>
              </div>

              <div v-if="skill?.permissions" class="space-y-2">
                <h4 class="font-medium text-sm">Required Permissions</h4>
                <div class="flex flex-wrap gap-2">
                  <Badge
                    v-for="perm in skill.permissions.required"
                    :key="typeof perm === 'string' ? perm : JSON.stringify(perm)"
                    variant="outline"
                  >
                    {{ typeof perm === 'string' ? perm : (perm as any).Custom || 'custom' }}
                  </Badge>
                </div>
              </div>
            </div>
            <p v-else class="text-muted-foreground text-center py-8">
              No requirements information available
            </p>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="versions" class="flex-1 mt-4 min-h-0">
          <ScrollArea class="h-[400px] rounded-md border p-4">
            <div v-if="loading" class="space-y-2">
              <Skeleton class="h-8 w-full" />
              <Skeleton class="h-8 w-full" />
            </div>
            <div v-else-if="versions.length" class="space-y-2">
              <div
                v-for="version in versions"
                :key="versionKey(version)"
                class="flex items-center justify-between p-2 rounded-md hover:bg-muted cursor-pointer"
                :class="{ 'bg-muted': selectedVersion === versionKey(version) }"
                @click="selectedVersion = versionKey(version)"
              >
                <span class="font-mono"> v{{ versionKey(version) }} </span>
                <Badge v-if="selectedVersion === versionKey(version)" variant="default">
                  Selected
                </Badge>
              </div>
            </div>
            <p v-else class="text-muted-foreground text-center py-8">No versions available</p>
          </ScrollArea>
        </TabsContent>
      </Tabs>

      <div class="flex items-center justify-between pt-4 border-t">
        <div class="flex items-center gap-2">
          <Button v-if="skill?.repository" variant="outline" size="sm" @click="openRepository">
            <ExternalLink class="w-4 h-4 mr-1" />
            Repository
          </Button>
          <Button v-if="skill?.homepage" variant="outline" size="sm" @click="openHomepage">
            <ExternalLink class="w-4 h-4 mr-1" />
            Homepage
          </Button>
        </div>
        <div class="flex items-center gap-2">
          <Button variant="outline" @click="handleClose">Cancel</Button>
          <Button
            v-if="!installed"
            :disabled="installing || (gatingResult && !gatingResult.passed)"
            @click="handleInstall"
          >
            <Download class="w-4 h-4 mr-1" />
            {{ installing ? 'Installing...' : 'Install' }}
          </Button>
          <Button v-else variant="secondary" disabled>
            <Check class="w-4 h-4 mr-1" />
            Installed
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
