import { ref } from 'vue'
import { listSecrets } from '@/api/secrets'
import type { Secret } from '@/types/generated/Secret'

export function useSecretsData() {
  const secrets = ref<Secret[]>([])
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  async function loadSecrets() {
    isLoading.value = true
    error.value = null
    try {
      secrets.value = await listSecrets()
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to load secrets'
      console.error('Failed to load secrets:', err)
    } finally {
      isLoading.value = false
    }
  }

  return {
    secrets,
    isLoading,
    error,
    loadSecrets,
  }
}
