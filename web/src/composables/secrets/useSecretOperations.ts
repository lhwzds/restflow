import {
  createSecret as apiCreateSecret,
  updateSecret as apiUpdateSecret,
  deleteSecret as apiDeleteSecret,
} from '@/api/secrets'
import type { Secret } from '@/types/generated/Secret'

export function useSecretOperations() {
  async function createSecret(key: string, value: string, description?: string): Promise<Secret> {
    try {
      const secret = await apiCreateSecret(key, value, description)
      return secret
    } catch (error) {
      console.error('Failed to create secret:', error)
      throw error
    }
  }

  async function updateSecret(key: string, value: string, description?: string) {
    try {
      await apiUpdateSecret(key, value, description)
      return true
    } catch (error) {
      console.error('Failed to update secret:', error)
      throw error
    }
  }

  async function deleteSecret(key: string) {
    try {
      await apiDeleteSecret(key)
      return true
    } catch (error) {
      console.error('Failed to delete secret:', error)
      throw error
    }
  }

  return {
    createSecret,
    updateSecret,
    deleteSecret,
  }
}
