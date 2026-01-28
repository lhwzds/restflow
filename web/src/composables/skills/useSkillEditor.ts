import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { getSkill, updateSkill, deleteSkill, exportSkill } from '@/api/skills'
import type { Skill } from '@/types/generated/Skill'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { downloadAsFile } from '@/utils/download'

export function useSkillEditor(skillId: string) {
  const router = useRouter()
  const toast = useToast()
  const { confirm } = useConfirm()

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
      formData.value.name !== skill.value.name || formData.value.content !== skill.value.content
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
      toast.success('Skill saved successfully')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save skill'
      toast.error(message)
      return false
    } finally {
      isSaving.value = false
    }
  }

  // Delete skill
  async function handleDelete(): Promise<boolean> {
    if (!skill.value) return false

    const confirmed = await confirm({
      title: 'Delete Skill',
      description: 'Are you sure you want to delete this skill?',
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'destructive',
    })

    if (!confirmed) return false

    try {
      await deleteSkill(skill.value.id)
      toast.success('Skill deleted successfully')
      router.push('/skills')
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete skill'
      toast.error(message)
      return false
    }
  }

  // Export skill
  async function handleExport(): Promise<void> {
    if (!skill.value) return

    try {
      const result = await exportSkill(skill.value.id)
      downloadAsFile(result.markdown, result.filename, 'text/markdown')
      toast.success('Skill exported successfully')
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to export skill'
      toast.error(message)
    }
  }

  // Navigate back with unsaved changes check
  async function goBack(): Promise<void> {
    if (hasChanges.value) {
      const confirmed = await confirm({
        title: 'Unsaved Changes',
        description: 'You have unsaved changes. Are you sure you want to leave?',
        confirmText: 'Leave',
        cancelText: 'Stay',
        variant: 'destructive',
      })
      if (confirmed) {
        router.push('/skills')
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
