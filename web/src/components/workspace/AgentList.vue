<script setup lang="ts">
import { Bot, MoreHorizontal, Plus, Trash2 } from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import type { AgentFile } from '@/types/workspace'

defineProps<{
  agents: AgentFile[]
  selectedAgentId: string | null
}>()

const emit = defineEmits<{
  select: [id: string]
  create: []
  delete: [id: string, name: string]
}>()

const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col min-h-0 bg-muted/30">
    <div class="px-3 pt-2 pb-3">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('create')">
        <Plus :size="16" />
        <span>{{ t('workspace.agent.create') }}</span>
      </Button>
    </div>

    <div class="flex-1 overflow-auto py-2">
      <div
        v-for="agent in agents"
        :key="agent.id"
        :data-testid="`agent-row-${agent.id}`"
        :class="
          cn(
            'group relative w-full cursor-pointer px-3 py-2 text-left transition-colors hover:bg-muted/50',
            selectedAgentId === agent.id && 'bg-muted',
          )
        "
        @click="emit('select', agent.id)"
      >
        <div class="flex items-start gap-2">
          <Bot :size="14" class="mt-0.5 text-muted-foreground" />

          <div class="min-w-0 flex-1">
            <div class="truncate text-sm">{{ agent.name }}</div>
            <div class="truncate text-xs text-muted-foreground font-mono">{{ agent.id }}</div>
          </div>

          <div class="absolute right-1 top-1" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="rounded p-1 opacity-0 transition-opacity group-hover:opacity-100 hover:bg-muted-foreground/10"
                >
                  <MoreHorizontal :size="14" class="text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" class="w-44">
                <DropdownMenuItem
                  class="text-destructive focus:text-destructive"
                  @click="emit('delete', agent.id, agent.name)"
                >
                  <Trash2 :size="14" class="mr-2" />
                  {{ t('workspace.agent.deleteWithName', { name: agent.name }) }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </div>

      <div v-if="agents.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
        {{ t('workspace.agent.empty') }}
      </div>
    </div>
  </div>
</template>
