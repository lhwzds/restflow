<script setup lang="ts">
import { Bot } from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { AgentFile } from '@/types/workspace'

defineProps<{
  selectedAgent: string | null
  availableAgents: AgentFile[]
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:selectedAgent': [value: string | null]
}>()

const { t } = useI18n()
</script>

<template>
  <Select
    :model-value="selectedAgent || ''"
    :disabled="disabled"
    @update:model-value="emit('update:selectedAgent', $event || null)"
  >
    <SelectTrigger class="w-[180px] h-8 text-xs">
      <Bot :size="14" class="mr-1 text-muted-foreground shrink-0" />
      <SelectValue :placeholder="t('common.agent')" />
    </SelectTrigger>
    <SelectContent>
      <SelectItem v-for="agent in availableAgents" :key="agent.id" :value="agent.id">
        {{ agent.name }}
      </SelectItem>
    </SelectContent>
  </Select>
</template>
