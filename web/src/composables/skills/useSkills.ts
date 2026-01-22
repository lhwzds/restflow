import { ref } from 'vue'
import {
  listSkills,
  createSkill,
  updateSkill,
  deleteSkill,
  exportSkill,
  type CreateSkillRequest,
  type UpdateSkillRequest,
} from '@/api/skills'
import type { Skill } from '@/types/generated/Skill'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { downloadAsFile } from '@/utils/download'

export function useSkills() {
  const toast = useToast()
  const { confirm } = useConfirm()
  const skills = ref<Skill[]>([])
  const isLoading = ref(false)
  const error = ref<string | null>(null)
  const selectedSkill = ref<Skill | null>(null)
  const isDialogVisible = ref(false)
  const isCreating = ref(false)

  // Load all skills
  async function loadSkills() {
    isLoading.value = true
    error.value = null
    try {
      skills.value = await listSkills()
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to load skills'
      console.error('Failed to load skills:', err)
    } finally {
      isLoading.value = false
    }
  }

  // Open dialog for creating new skill
  function openCreateDialog() {
    selectedSkill.value = null
    isCreating.value = true
    isDialogVisible.value = true
  }

  // Open dialog for editing existing skill
  function openEditDialog(skill: Skill) {
    selectedSkill.value = { ...skill }
    isCreating.value = false
    isDialogVisible.value = true
  }

  // Close dialog
  function closeDialog() {
    isDialogVisible.value = false
    selectedSkill.value = null
    isCreating.value = false
  }

  // Create a new skill
  async function handleCreate(request: CreateSkillRequest): Promise<Skill | null> {
    try {
      const newSkill = await createSkill(request)
      skills.value.unshift(newSkill)
      toast.success('Skill created successfully')
      closeDialog()
      return newSkill
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create skill'
      toast.error(message)
      return null
    }
  }

  // Update an existing skill
  async function handleUpdate(id: string, request: UpdateSkillRequest): Promise<boolean> {
    try {
      const updatedSkill = await updateSkill(id, request)
      const index = skills.value.findIndex((s) => s.id === id)
      if (index !== -1) {
        skills.value[index] = updatedSkill
      }
      toast.success('Skill updated successfully')
      closeDialog()
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update skill'
      toast.error(message)
      return false
    }
  }

  // Delete a skill
  async function handleDelete(id: string): Promise<boolean> {
    const confirmed = await confirm({
      title: 'Delete Skill',
      description: 'Are you sure you want to delete this skill?',
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'destructive',
    })

    if (!confirmed) return false

    try {
      await deleteSkill(id)
      skills.value = skills.value.filter((s) => s.id !== id)
      toast.success('Skill deleted successfully')
      closeDialog()
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete skill'
      toast.error(message)
      return false
    }
  }

  // Export a skill to markdown
  async function handleExport(id: string): Promise<void> {
    try {
      const result = await exportSkill(id)
      downloadAsFile(result.markdown, result.filename, 'text/markdown')
      toast.success('Skill exported successfully')
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to export skill'
      toast.error(message)
    }
  }

  return {
    // State
    skills,
    isLoading,
    error,
    selectedSkill,
    isDialogVisible,
    isCreating,
    // Actions
    loadSkills,
    openCreateDialog,
    openEditDialog,
    closeDialog,
    handleCreate,
    handleUpdate,
    handleDelete,
    handleExport,
  }
}
