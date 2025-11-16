import { defineStore } from 'pinia'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'
import type { AIModel } from '@/types/generated/AIModel'
import type { Provider } from '@/types/generated/Provider'
import { API_ENDPOINTS } from '@/constants/api/endpoints'

interface ModelsState {
  models: ModelMetadataDTO[]
  loaded: boolean
  loading: boolean
  error: string | null
}

export const useModelsStore = defineStore('models', {
  state: (): ModelsState => ({
    models: [],
    loaded: false,
    loading: false,
    error: null,
  }),

  getters: {
    /**
     * Get metadata for a specific model
     */
    getModelMetadata: (state) => (model: AIModel): ModelMetadataDTO | undefined => {
      return state.models.find((m) => m.model === model)
    },

    /**
     * Get all models for a specific provider
     */
    getModelsByProvider: (state) => (provider: Provider): ModelMetadataDTO[] => {
      return state.models.filter((m) => m.provider === provider)
    },

    /**
     * Get all available models
     */
    getAllModels: (state): ModelMetadataDTO[] => state.models,

    /**
     * Check if models are ready to use
     */
    isReady: (state): boolean => state.loaded && !state.loading && !state.error,
  },

  actions: {
    /**
     * Load models from the API
     */
    async loadModels() {
      // Skip if already loaded or loading
      if (this.loaded || this.loading) {
        return
      }

      this.loading = true
      this.error = null

      try {
        const response = await fetch(API_ENDPOINTS.MODEL.LIST)

        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${response.statusText}`)
        }

        const result = await response.json()

        if (result.success && result.data) {
          this.models = result.data
          this.loaded = true
        } else {
          throw new Error(result.message || 'Invalid response format')
        }
      } catch (error) {
        this.error = error instanceof Error ? error.message : 'Unknown error'
        console.error('[ModelsStore] Failed to load models:', error)
        // Re-throw error so plugin can catch it and show notification
        throw error
      } finally {
        this.loading = false
      }
    },

    /**
     * Force reload models from the API
     */
    async reloadModels() {
      this.loaded = false
      await this.loadModels()
    },
  },
})
