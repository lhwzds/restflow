import { ref, computed } from 'vue'
import { listSecrets } from '@/api/secrets'
import type { Secret } from '@/types/generated/Secret'

export function useSecretsList() {
  const secrets = ref<Secret[]>([])
  const isLoading = ref(false)
  const searchQuery = ref('')

  const filteredSecrets = computed(() => {
    if (!searchQuery.value) {
      return secrets.value
    }

    const query = searchQuery.value.toLowerCase()
    return secrets.value.filter(
      (secret) =>
        secret.key.toLowerCase().includes(query) ||
        (secret.description ?? '').toLowerCase().includes(query),
    )
  })

  async function loadSecrets() {
    isLoading.value = true
    try {
      const response = await listSecrets()
      secrets.value = response
    } catch (error) {
      console.error('Failed to load secrets:', error)
      secrets.value = []
    } finally {
      isLoading.value = false
    }
  }

  return {
    secrets,
    isLoading,
    searchQuery,
    filteredSecrets,
    loadSecrets,
  }
}
