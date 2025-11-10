import { readonly } from 'vue'
import { MODEL_OPTIONS } from '@/constants/node/models'

/**
 * Agent models management composable
 * Provides unified model list and helper functions
 */
export function useAgentModels() {
  // Build AVAILABLE_MODELS from MODEL_OPTIONS with supportsTemperature flag
  const AVAILABLE_MODELS = readonly(
    MODEL_OPTIONS.map((option) => ({
      label: option.label,
      value: option.value,
      // O-series models don't support temperature
      supportsTemperature: !option.value.startsWith('o'),
    })),
  )

  const O_SERIES_MODELS = ['o4-mini', 'o3', 'o3-mini']

  /**
   * Check if model is O-series
   */
  const isOSeriesModel = (model: string): boolean => {
    return O_SERIES_MODELS.includes(model)
  }

  /**
   * Get default temperature for model
   * Returns null for O-series models, 0.7 for others
   */
  const getDefaultTemperature = (model: string): number | null => {
    return isOSeriesModel(model) ? null : 0.7
  }

  /**
   * Check if model supports temperature configuration
   */
  const supportsTemperature = (model: string): boolean => {
    const modelInfo = AVAILABLE_MODELS.find((m) => m.value === model)
    return modelInfo?.supportsTemperature ?? true
  }

  /**
   * Get model display label
   */
  const getModelLabel = (model: string): string => {
    const modelInfo = AVAILABLE_MODELS.find((m) => m.value === model)
    return modelInfo?.label || model
  }

  return {
    AVAILABLE_MODELS,
    O_SERIES_MODELS,
    isOSeriesModel,
    getDefaultTemperature,
    supportsTemperature,
    getModelLabel,
  }
}
