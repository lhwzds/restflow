import { readonly } from 'vue'
import { MODEL_OPTIONS } from '@/constants/node/models'

/**
 * Agent models management composable
 * Provides unified model list and helper functions
 */
export function useAgentModels() {
  // Models that don't support temperature parameter
  const NO_TEMPERATURE_MODELS = [
    'o4-mini',
    'o3',
    'o3-mini',
    'gpt-5',
    'gpt-5-mini',
    'gpt-5-nano',
    'gpt-5-pro',
  ]

  // Build AVAILABLE_MODELS from MODEL_OPTIONS with supportsTemperature flag
  const AVAILABLE_MODELS = readonly(
    MODEL_OPTIONS.map((option) => ({
      label: option.label,
      value: option.value,
      // O-series and GPT-5 series models don't support temperature
      supportsTemperature: !NO_TEMPERATURE_MODELS.includes(option.value),
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
   * Returns null for models that don't support temperature, 0.7 for others
   */
  const getDefaultTemperature = (model: string): number | null => {
    return NO_TEMPERATURE_MODELS.includes(model) ? null : 0.7
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
