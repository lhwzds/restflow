import { useModelsStore } from '@/stores/modelsStore'
import type { ModelId } from '@/types/generated/ModelId'
import type { Provider } from '@/types/generated/Provider'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'
import { getProviderDisplayName } from '@/utils/providerCatalog'

// Model option interface for UI components
export interface ModelOption {
  value: ModelId
  label: string
  provider: Provider
  supportsTemperature: boolean
}

/**
 * Get metadata for a specific model
 * Delegates to the models store which fetches from backend
 */
function getMetadata(model: ModelId): ModelMetadataDTO | undefined {
  const store = useModelsStore()
  return store.getModelMetadata(model)
}

/**
 * Get provider for a model
 */
export function getProvider(model: ModelId): Provider | undefined {
  const metadata = getMetadata(model)
  return metadata?.provider
}

/**
 * Check if model supports temperature parameter
 */
export function supportsTemperature(model: ModelId): boolean {
  const metadata = getMetadata(model)
  return metadata?.supports_temperature ?? false
}

/**
 * Get display name for model
 */
export function getModelDisplayName(model: ModelId): string {
  const metadata = getMetadata(model)
  return metadata?.name || model
}

/**
 * Get default temperature for a model
 */
export function getDefaultTemperature(model: ModelId): number | undefined {
  return supportsTemperature(model) ? 0.7 : undefined
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
  const allModelsGetter = store.getAllModels as ModelMetadataDTO[] | (() => ModelMetadataDTO[])
  const metadataList = typeof allModelsGetter === 'function' ? allModelsGetter() : allModelsGetter

  return (metadataList ?? []).map((m) => ({
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

export { getProviderDisplayName }
