<script setup lang="ts">
import { computed } from 'vue'
import { Pencil, Tag, FileText, Bot, X } from 'lucide-vue-next'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import type { FileItem } from '@/types/workspace'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const props = defineProps<{
  item: FileItem<Skill | StoredAgent> | null
  type: 'skill' | 'agent'
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  edit: []
}>()

// Check if the item data is a Skill
function isSkill(data: unknown): data is Skill {
  return data !== null && typeof data === 'object' && 'content' in data
}

// Check if the item data is a StoredAgent
function isAgent(data: unknown): data is StoredAgent {
  return data !== null && typeof data === 'object' && 'agent' in data
}

// Render Markdown content
const renderedContent = computed(() => {
  if (!props.item?.data) return ''

  let content = ''
  if (isSkill(props.item.data)) {
    content = props.item.data.content || ''
  } else if (isAgent(props.item.data)) {
    content = props.item.data.agent.prompt || '*No system prompt configured*'
  }

  const html = marked.parse(content, { async: false }) as string
  return DOMPurify.sanitize(html)
})

// Get tags for skills
const tags = computed(() => {
  if (!props.item?.data || !isSkill(props.item.data)) return []
  return props.item.data.tags || []
})

// Get description
const description = computed(() => {
  if (!props.item?.data) return ''
  if (isSkill(props.item.data)) {
    return props.item.data.description || ''
  }
  return ''
})

// Get agent info
const agentInfo = computed(() => {
  if (!props.item?.data || !isAgent(props.item.data)) return null
  return {
    model: props.item.data.agent.model,
    temperature: props.item.data.agent.temperature,
    tools: props.item.data.agent.tools || [],
  }
})

const handleEdit = () => {
  emit('update:open', false)
  emit('edit')
}

const handleClose = () => {
  emit('update:open', false)
}
</script>

<template>
  <!-- Floating preview card - positioned absolutely in parent -->
  <Transition
    enter-active-class="transition-all duration-200 ease-out"
    enter-from-class="opacity-0 translate-x-4"
    enter-to-class="opacity-100 translate-x-0"
    leave-active-class="transition-all duration-150 ease-in"
    leave-from-class="opacity-100 translate-x-0"
    leave-to-class="opacity-0 translate-x-4"
  >
    <Card
      v-if="open && item"
      class="absolute right-4 top-16 w-72 max-h-[calc(100%-5rem)] flex flex-col shadow-lg z-10"
    >
      <!-- Header -->
      <div class="px-3 py-2 border-b flex items-center gap-2">
        <FileText v-if="type === 'skill'" :size="16" class="text-muted-foreground shrink-0" />
        <Bot v-else :size="16" class="text-muted-foreground shrink-0" />
        <span class="font-medium text-sm truncate flex-1">{{ item.name }}</span>
        <Button variant="ghost" size="icon" class="h-6 w-6 shrink-0" @click="handleClose">
          <X :size="14" />
        </Button>
      </div>

      <!-- Description -->
      <div v-if="description" class="px-3 py-1.5 text-xs text-muted-foreground border-b">
        {{ description }}
      </div>

      <!-- Tags (for skills) -->
      <div v-if="tags.length > 0" class="px-3 py-1.5 border-b flex items-center gap-1.5 flex-wrap">
        <Tag :size="12" class="text-muted-foreground shrink-0" />
        <Badge v-for="tag in tags" :key="tag" variant="secondary" class="text-[10px] px-1.5 py-0">
          {{ tag }}
        </Badge>
      </div>

      <!-- Agent Info (for agents) -->
      <div
        v-if="agentInfo"
        class="px-3 py-1.5 border-b text-[10px] text-muted-foreground space-y-0.5"
      >
        <div><strong>Model:</strong> {{ agentInfo.model }}</div>
        <div v-if="agentInfo.temperature !== undefined">
          <strong>Temperature:</strong> {{ agentInfo.temperature }}
        </div>
        <div v-if="agentInfo.tools.length > 0">
          <strong>Tools:</strong> {{ agentInfo.tools.join(', ') }}
        </div>
      </div>

      <!-- Content Preview -->
      <div class="flex-1 overflow-auto px-3 py-2 min-h-[80px]">
        <div v-html="renderedContent" class="prose prose-xs dark:prose-invert max-w-none text-xs" />
      </div>

      <!-- Footer -->
      <div class="px-3 py-2 border-t flex justify-end">
        <Button size="sm" class="h-7" @click="handleEdit">
          <Pencil :size="12" class="mr-1" />
          Edit
        </Button>
      </div>
    </Card>
  </Transition>
</template>
