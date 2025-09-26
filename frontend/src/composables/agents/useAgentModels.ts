import { readonly } from 'vue'

/**
 * Agent models management composable
 * Provides unified model list and helper functions
 */
export function useAgentModels() {
  const AVAILABLE_MODELS = readonly([
    // OpenAI O Series (Reasoning models) - no temperature support
    { label: 'O4 Mini', value: 'o4-mini', supportsTemperature: false },
    { label: 'O3', value: 'o3', supportsTemperature: false },
    { label: 'O3 Mini', value: 'o3-mini', supportsTemperature: false },
    // GPT series
    { label: 'GPT-4.1', value: 'gpt-4.1', supportsTemperature: true },
    { label: 'GPT-4.1 Mini', value: 'gpt-4.1-mini', supportsTemperature: true },
    { label: 'GPT-4.1 Nano', value: 'gpt-4.1-nano', supportsTemperature: true },
    // Claude series
    { label: 'Claude 4 Opus', value: 'claude-4-opus', supportsTemperature: true },
    { label: 'Claude 4 Sonnet', value: 'claude-4-sonnet', supportsTemperature: true },
    { label: 'Claude 3.7 Sonnet', value: 'claude-3.7-sonnet', supportsTemperature: true },
    // DeepSeek series
    { label: 'DeepSeek Chat', value: 'deepseek-chat', supportsTemperature: true },
    { label: 'DeepSeek Reasoner', value: 'deepseek-reasoner', supportsTemperature: true },
  ])

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
    const modelInfo = AVAILABLE_MODELS.find(m => m.value === model)
    return modelInfo?.supportsTemperature ?? true
  }

  /**
   * Get model display label
   */
  const getModelLabel = (model: string): string => {
    const modelInfo = AVAILABLE_MODELS.find(m => m.value === model)
    return modelInfo?.label || model
  }

  return {
    AVAILABLE_MODELS,
    O_SERIES_MODELS,
    isOSeriesModel,
    getDefaultTemperature,
    supportsTemperature,
    getModelLabel
  }
}