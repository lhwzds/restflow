import { ref, computed, watch } from 'vue'
import { useRouter } from 'vue-router'
import { getSkill, updateSkill, deleteSkill, exportSkill } from '@/api/skills'
import type { Skill } from '@/types/generated/Skill'
import { ElMessage, ElMessageBox } from 'element-plus'

export function useSkillEditor(skillId: string) {
  const router = useRouter()

  const skill = ref<Skill | null>(null)
  const isLoading = ref(false)
  const isSaving = ref(false)
  const error = ref<string | null>(null)

  // Editable form data
  const formData = ref({
    name: '',
    content: '',
  })

  // Track if there are unsaved changes
  const hasChanges = computed(() => {
    if (!skill.value) return false
    return (
      formData.value.name !== skill.value.name ||
      formData.value.content !== skill.value.content
    )
  })

  // Load skill data
  async function loadSkill() {
    isLoading.value = true
    error.value = null
    try {
      const data = await getSkill(skillId)
      skill.value = data
      // Initialize form data
      formData.value = {
        name: data.name,
        content: data.content,
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to load skill'
      console.error('Failed to load skill:', err)
    } finally {
      isLoading.value = false
    }
  }

  // Save changes
  async function saveSkill(): Promise<boolean> {
    if (!skill.value || !hasChanges.value) return true

    isSaving.value = true
    try {
      await updateSkill(skill.value.id, {
        name: formData.value.name,
        content: formData.value.content,
      })
      // Update local skill data
      skill.value = {
        ...skill.value,
        name: formData.value.name,
        content: formData.value.content,
      }
      ElMessage.success('Skill saved successfully')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save skill'
      ElMessage.error(message)
      return false
    } finally {
      isSaving.value = false
    }
  }

  // Delete skill
  async function handleDelete(): Promise<boolean> {
    if (!skill.value) return false

    try {
      await ElMessageBox.confirm('Are you sure you want to delete this skill?', 'Delete Skill', {
        confirmButtonText: 'Delete',
        cancelButtonText: 'Cancel',
        type: 'warning',
      })

      await deleteSkill(skill.value.id)
      ElMessage.success('Skill deleted successfully')
      router.push('/skills')
      return true
    } catch (err) {
      if (err !== 'cancel') {
        const message = err instanceof Error ? err.message : 'Failed to delete skill'
        ElMessage.error(message)
      }
      return false
    }
  }

  // Export skill
  async function handleExport(): Promise<void> {
    if (!skill.value) return

    try {
      const result = await exportSkill(skill.value.id)
      // Create download link
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

  // Navigate back with unsaved changes check
  async function goBack(): Promise<void> {
    if (hasChanges.value) {
      try {
        await ElMessageBox.confirm(
          'You have unsaved changes. Are you sure you want to leave?',
          'Unsaved Changes',
          {
            confirmButtonText: 'Leave',
            cancelButtonText: 'Stay',
            type: 'warning',
          }
        )
        router.push('/skills')
      } catch {
        // User cancelled, stay on page
      }
    } else {
      router.push('/skills')
    }
  }

  return {
    // State
    skill,
    formData,
    isLoading,
    isSaving,
    error,
    hasChanges,
    // Actions
    loadSkill,
    saveSkill,
    handleDelete,
    handleExport,
    goBack,
  }
}
