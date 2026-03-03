<script setup lang="ts">
/**
 * SystemSection Component
 *
 * Exposes backend system utilities that are currently not reachable from
 * other settings sections.
 */
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { getSystemConfig, hasSecretKey, updateSystemConfig } from '@/api/config'
import { importSkillFromJson } from '@/api/skills'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()

const configText = ref('')
const configLoading = ref(false)
const configSaving = ref(false)

const secretKey = ref('')
const checkingSecret = ref(false)
const secretExists = ref<boolean | null>(null)

const skillJson = ref('')
const importingSkill = ref(false)

async function loadConfig() {
  configLoading.value = true
  try {
    const config = await getSystemConfig()
    configText.value = JSON.stringify(config, null, 2)
    toast.success(t('settings.system.configLoaded'))
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    toast.error(message)
  } finally {
    configLoading.value = false
  }
}

async function saveConfig() {
  let parsed: Record<string, unknown>
  try {
    parsed = JSON.parse(configText.value)
  } catch {
    toast.error(t('settings.system.invalidJson'))
    return
  }

  const confirmed = await confirm({
    title: t('settings.system.saveConfig'),
    description: t('settings.system.description'),
    confirmText: t('common.confirm'),
    cancelText: t('common.cancel'),
  })
  if (!confirmed) return

  configSaving.value = true
  try {
    const updated = await updateSystemConfig(parsed)
    configText.value = JSON.stringify(updated, null, 2)
    toast.success(t('settings.system.configSaved'))
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    toast.error(message)
  } finally {
    configSaving.value = false
  }
}

async function checkSecret() {
  const key = secretKey.value.trim()
  if (!key) return

  checkingSecret.value = true
  try {
    secretExists.value = await hasSecretKey(key)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    toast.error(message)
    secretExists.value = null
  } finally {
    checkingSecret.value = false
  }
}

async function importSkill() {
  const json = skillJson.value.trim()
  if (!json) return

  const confirmed = await confirm({
    title: t('settings.system.importSkill'),
    description: t('settings.system.skillImportTitle'),
    confirmText: t('common.confirm'),
    cancelText: t('common.cancel'),
  })
  if (!confirmed) return

  importingSkill.value = true
  try {
    const skill = await importSkillFromJson(json)
    toast.success(t('settings.system.importSuccess', { name: skill.name }))
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    toast.error(message)
  } finally {
    importingSkill.value = false
  }
}

onMounted(() => {
  void loadConfig()
})
</script>

<template>
  <div data-testid="system-section" class="space-y-4">
    <div>
      <h2 class="text-2xl font-bold tracking-tight">{{ t('settings.system.title') }}</h2>
      <p class="text-muted-foreground">{{ t('settings.system.description') }}</p>
    </div>

    <section class="rounded-lg border bg-card p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-base font-semibold">{{ t('settings.system.configEditor') }}</h3>
        <div class="flex items-center gap-2">
          <Button variant="outline" :disabled="configLoading" @click="loadConfig">
            <Loader2 v-if="configLoading" class="mr-1 h-3 w-3 animate-spin" />
            {{ t('settings.system.loadConfig') }}
          </Button>
          <Button :disabled="configSaving" @click="saveConfig">
            <Loader2 v-if="configSaving" class="mr-1 h-3 w-3 animate-spin" />
            {{ t('settings.system.saveConfig') }}
          </Button>
        </div>
      </div>
      <Textarea
        v-model="configText"
        rows="12"
        class="font-mono text-xs"
        :placeholder="t('settings.system.configPlaceholder')"
      />
    </section>

    <section class="rounded-lg border bg-card p-4 space-y-3">
      <h3 class="text-base font-semibold">{{ t('settings.system.secretCheckTitle') }}</h3>
      <div class="grid gap-2">
        <Label for="system-secret-key">{{ t('settings.system.secretKeyLabel') }}</Label>
        <div class="flex items-center gap-2">
          <Input
            id="system-secret-key"
            v-model="secretKey"
            :placeholder="t('settings.system.secretKeyPlaceholder')"
            @keydown.enter.prevent="checkSecret"
          />
          <Button variant="outline" :disabled="checkingSecret" @click="checkSecret">
            <Loader2 v-if="checkingSecret" class="mr-1 h-3 w-3 animate-spin" />
            {{ t('settings.system.checkSecret') }}
          </Button>
        </div>
      </div>
      <Badge v-if="secretExists === true" variant="default">{{ t('settings.system.secretExists') }}</Badge>
      <Badge v-else-if="secretExists === false" variant="secondary">{{ t('settings.system.secretMissing') }}</Badge>
    </section>

    <section class="rounded-lg border bg-card p-4 space-y-3">
      <h3 class="text-base font-semibold">{{ t('settings.system.skillImportTitle') }}</h3>
      <div class="grid gap-2">
        <Label for="system-skill-json">{{ t('settings.system.skillImportLabel') }}</Label>
        <Textarea
          id="system-skill-json"
          v-model="skillJson"
          rows="8"
          class="font-mono text-xs"
          :placeholder="t('settings.system.skillImportPlaceholder')"
        />
      </div>
      <Button :disabled="importingSkill" @click="importSkill">
        <Loader2 v-if="importingSkill" class="mr-1 h-3 w-3 animate-spin" />
        {{ t('settings.system.importSkill') }}
      </Button>
    </section>
  </div>
</template>
