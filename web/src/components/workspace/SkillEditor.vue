<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { X, Save, Loader2, FileText } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { createSkill, updateSkill } from '@/api/skills'
import type { Skill } from '@/types/generated/Skill'
import { useToast } from '@/composables/useToast'
import { skillToMarkdown, parseMarkdown, newSkillTemplate } from '@/utils/skillMarkdown'

const props = withDefaults(
  defineProps<{
    skill?: Skill | null
    isNew?: boolean
    showHeader?: boolean
  }>(),
  {
    showHeader: true,
  },
)

const emit = defineEmits<{
  save: [skill: Skill]
  cancel: []
}>()

const toast = useToast()
const isSaving = ref(false)

// File name (editable)
const fileName = ref('Untitled')

// Raw markdown content (includes frontmatter)
const rawContent = ref('')

// Initialize content from skill
watch(
  () => props.skill,
  (skill) => {
    if (skill) {
      fileName.value = skill.name
      rawContent.value = skillToMarkdown(skill)
    } else {
      fileName.value = 'Untitled'
      rawContent.value = newSkillTemplate
    }
  },
  { immediate: true },
)

// Check if can save
const canSave = computed(() => {
  // For new skill: can save if file has a name (not 'Untitled')
  if (!props.skill) {
    return fileName.value.trim() !== '' && fileName.value !== 'Untitled'
  }
  // For existing skill: can save if there are changes
  return fileName.value !== props.skill.name || rawContent.value !== skillToMarkdown(props.skill)
})

// Save the skill
async function handleSave() {
  const name = fileName.value.trim()
  if (!name || name === 'Untitled') {
    toast.error('Please enter a file name')
    return
  }

  const parsed = parseMarkdown(rawContent.value)

  isSaving.value = true
  try {
    let savedSkill: Skill

    if (props.isNew || !props.skill) {
      savedSkill = await createSkill({
        name,
        description: parsed.description,
        tags: parsed.tags,
        content: parsed.body,
      })
      toast.success('Skill created')
    } else {
      savedSkill = await updateSkill(props.skill.id, {
        name,
        description: parsed.description,
        tags: parsed.tags,
        content: parsed.body,
      })
      toast.success('Skill saved')
    }

    emit('save', savedSkill)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to save'
    toast.error(message)
  } finally {
    isSaving.value = false
  }
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Header: File name + actions (conditional) -->
    <div v-if="showHeader" class="h-11 border-b flex items-center px-3 gap-3 shrink-0">
      <FileText :size="18" class="text-muted-foreground shrink-0" />

      <!-- Editable file name -->
      <Input
        v-model="fileName"
        class="h-7 text-sm font-medium border-none shadow-none focus-visible:ring-0 px-1 bg-transparent"
        :class="{ 'text-muted-foreground italic': fileName === 'Untitled' }"
        placeholder="Enter file name..."
      />

      <span class="text-muted-foreground text-sm">.md</span>

      <div class="flex-1" />

      <!-- Actions -->
      <Button variant="ghost" size="sm" class="h-7" :disabled="isSaving" @click="emit('cancel')">
        <X :size="14" class="mr-1" />
        Cancel
      </Button>
      <Button size="sm" class="h-7" :disabled="isSaving || !canSave" @click="handleSave">
        <Loader2 v-if="isSaving" :size="14" class="mr-1 animate-spin" />
        <Save v-else :size="14" class="mr-1" />
        Save
      </Button>
    </div>

    <!-- Markdown Editor (full height) -->
    <Textarea
      v-model="rawContent"
      class="flex-1 resize-none border-0 rounded-none focus-visible:ring-0 font-mono text-sm p-4 bg-background"
      placeholder="Write your skill in Markdown..."
    />
  </div>
</template>
