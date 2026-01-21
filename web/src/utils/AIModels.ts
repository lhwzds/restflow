import { useModelsStore } from '@/stores/modelsStore'
import type { AIModel } from '@/types/generated/AIModel'
import type { Provider } from '@/types/generated/Provider'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'

// Model option interface for UI components
export interface ModelOption {
  value: AIModel
  label: string
  provider: Provider
  supportsTemperature: boolean
}

/**
 * Get metadata for a specific model
 * Delegates to the models store which fetches from backend
 */
function getMetadata(model: AIModel): ModelMetadataDTO | undefined {
  const store = useModelsStore()
  return store.getModelMetadata(model)
}

/**
 * Get provider for a model
 */
export function getProvider(model: AIModel): Provider {
  const metadata = getMetadata(model)
  return metadata?.provider || 'openai'
}

/**
 * Check if model supports temperature parameter
 */
export function supportsTemperature(model: AIModel): boolean {
  const metadata = getMetadata(model)
  return metadata?.supports_temperature ?? false
}

/**
 * Get display name for model
 */
export function getModelDisplayName(model: AIModel): string {
  const metadata = getMetadata(model)
  return metadata?.name || model
}

/**
 * Get default temperature for a model
 */
export function getDefaultTemperature(model: AIModel): number | null {
  return supportsTemperature(model) ? 0.7 : null
}

/**
 * Get models by provider
 */
export function getModelsByProvider(provider: Provider): ModelOption[] {
  const store = useModelsStore()
  return store.getModelsByProvider(provider).map((m) => ({
    value: m.model,
    label: m.name,
    provider: m.provider,
    supportsTemperature: m.supports_temperature,
  }))
}

/**
 * Get all available models as options
 */
export function getAllModels(): ModelOption[] {
  const store = useModelsStore()
  return store.getAllModels.map((m) => ({
    value: m.model,
    label: m.name,
    provider: m.provider,
    supportsTemperature: m.supports_temperature,
  }))
}

/**
 * Get Element Plus tag type for provider (for UI styling)
 */
export function getProviderTagType(provider: Provider): 'success' | 'warning' | 'info' | 'danger' {
  switch (provider) {
    case 'openai':
      return 'success'
    case 'anthropic':
      return 'warning'
    case 'deepseek':
      return 'info'
    default:
      return 'info'
  }
}
