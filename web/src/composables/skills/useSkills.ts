import { ref, computed } from 'vue'
import {
  listSkills,
  getSkill,
  createSkill,
  updateSkill,
  deleteSkill,
  exportSkill,
  importSkill,
  type CreateSkillRequest,
  type UpdateSkillRequest,
  type ImportSkillRequest,
} from '@/api/skills'
import type { Skill } from '@/types/generated/Skill'
import { ElMessage, ElMessageBox } from 'element-plus'

export function useSkills() {
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
  async function handleCreate(request: CreateSkillRequest): Promise<boolean> {
    try {
      const newSkill = await createSkill(request)
      skills.value.unshift(newSkill)
      ElMessage.success('Skill created successfully')
      closeDialog()
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create skill'
      ElMessage.error(message)
      return false
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
      ElMessage.success('Skill updated successfully')
      closeDialog()
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update skill'
      ElMessage.error(message)
      return false
    }
  }

  // Delete a skill
  async function handleDelete(id: string): Promise<boolean> {
    try {
      await ElMessageBox.confirm('Are you sure you want to delete this skill?', 'Delete Skill', {
        confirmButtonText: 'Delete',
        cancelButtonText: 'Cancel',
        type: 'warning',
      })

      await deleteSkill(id)
      skills.value = skills.value.filter((s) => s.id !== id)
      ElMessage.success('Skill deleted successfully')
      closeDialog()
      return true
    } catch (err) {
      if (err !== 'cancel') {
        const message = err instanceof Error ? err.message : 'Failed to delete skill'
        ElMessage.error(message)
      }
      return false
    }
  }

  // Export a skill to markdown
  async function handleExport(id: string): Promise<void> {
    try {
      const result = await exportSkill(id)
      // Create a download link
      const blob = new Blob([result.markdown], { type: 'text/markdown' })
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = result.filename
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
      URL.revokeObjectURL(url)
      ElMessage.success('Skill exported successfully')
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to export skill'
      ElMessage.error(message)
    }
  }

  // Import a skill from markdown
  async function handleImport(request: ImportSkillRequest): Promise<boolean> {
    try {
      const newSkill = await importSkill(request)
      skills.value.unshift(newSkill)
      ElMessage.success('Skill imported successfully')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to import skill'
      ElMessage.error(message)
      return false
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
    handleImport,
  }
}
