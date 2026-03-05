import { defineStore } from 'pinia'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'
import type { AIModel } from '@/types/generated/AIModel'
import type { Provider } from '@/types/generated/Provider'
import { tauriInvoke } from '@/api/config'

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
    getModelMetadata:
      (state) =>
      (model: AIModel): ModelMetadataDTO | undefined => {
        return state.models.find((m) => m.model === model)
      },

    /**
     * Get all models for a specific provider
     */
    getModelsByProvider:
      (state) =>
      (provider: Provider): ModelMetadataDTO[] => {
        return state.models.filter((m) => m.provider === provider)
      },

    /**
     * Get all unique providers that currently have available models.
     */
    getProviders: (state): Provider[] => {
      const providers = Array.from(new Set(state.models.map((m) => m.provider)))
      return providers.sort()
    },

    /**
     * Get the first model for a provider.
     */
    getFirstModelByProvider:
      (state) =>
      (provider: Provider): AIModel | undefined => {
        return state.models.find((m) => m.provider === provider)?.model
      },

    /**
     * Check whether a model belongs to a provider.
     */
    isModelInProvider:
      (state) =>
      (provider: Provider, model: AIModel): boolean => {
        return state.models.some((m) => m.provider === provider && m.model === model)
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
        this.models = await tauriInvoke<ModelMetadataDTO[]>('get_available_models')
        this.loaded = true
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
