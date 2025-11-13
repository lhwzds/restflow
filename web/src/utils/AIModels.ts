import type { AIModel } from '@/types/generated/AIModel'
import type { Provider } from '@/types/generated/Provider'

// Model metadata interface
export interface ModelOption {
  value: AIModel
  label: string
  provider: Provider
  supportsTemperature: boolean
}

// Get provider for a model
export function getProvider(model: AIModel): Provider {
  if (
    model === 'gpt-5' ||
    model === 'gpt-5-mini' ||
    model === 'gpt-5-nano' ||
    model === 'gpt-5-pro' ||
    model === 'o4-mini' ||
    model === 'o3' ||
    model === 'o3-mini'
  ) {
    return 'openai'
  }

  if (
    model === 'claude-opus-4-1' ||
    model === 'claude-sonnet-4-5' ||
    model === 'claude-haiku-4-5'
  ) {
    return 'anthropic'
  }

  if (model === 'deepseek-chat' || model === 'deepseek-reasoner') {
    return 'deepseek'
  }

  // Fallback (should not happen with proper typing)
  return 'openai'
}

// Check if model supports temperature parameter
export function supportsTemperature(model: AIModel): boolean {
  // O-series and GPT-5 series don't support temperature
  const NO_TEMPERATURE_MODELS: AIModel[] = [
    'o4-mini',
    'o3',
    'o3-mini',
    'gpt-5',
    'gpt-5-mini',
    'gpt-5-nano',
    'gpt-5-pro',
  ]

  return !NO_TEMPERATURE_MODELS.includes(model)
}

// Get display name for model
export function getModelDisplayName(model: AIModel): string {
  const displayNames: Record<AIModel, string> = {
    // GPT-5 series
    'gpt-5': 'GPT-5',
    'gpt-5-mini': 'GPT-5 Mini',
    'gpt-5-nano': 'GPT-5 Nano',
    'gpt-5-pro': 'GPT-5 Pro',

    // O-series
    'o4-mini': 'O4 Mini',
    o3: 'O3',
    'o3-mini': 'O3 Mini',

    // Claude series
    'claude-opus-4-1': 'Claude Opus 4.1',
    'claude-sonnet-4-5': 'Claude Sonnet 4.5',
    'claude-haiku-4-5': 'Claude Haiku 4.5',

    // DeepSeek series
    'deepseek-chat': 'DeepSeek Chat',
    'deepseek-reasoner': 'DeepSeek Reasoner',
  }

  return displayNames[model] || model
}

// Get Element Plus tag type based on provider
export function getProviderTagType(
  provider: Provider,
): 'success' | 'warning' | 'info' | 'primary' | 'danger' {
  switch (provider) {
    case 'openai':
      return 'success'
    case 'anthropic':
      return 'warning'
    case 'deepseek':
      return 'info'
  }
}

// Get all available models as options
export function getAllModels(): ModelOption[] {
  const allModels: AIModel[] = [
    // OpenAI
    'gpt-5',
    'gpt-5-mini',
    'gpt-5-nano',
    'gpt-5-pro',
    'o4-mini',
    'o3',
    'o3-mini',
    // Anthropic
    'claude-opus-4-1',
    'claude-sonnet-4-5',
    'claude-haiku-4-5',
    // DeepSeek
    'deepseek-chat',
    'deepseek-reasoner',
  ]

  return allModels.map((model) => ({
    value: model,
    label: getModelDisplayName(model),
    provider: getProvider(model),
    supportsTemperature: supportsTemperature(model),
  }))
}

// Get models by provider
export function getModelsByProvider(provider: Provider): ModelOption[] {
  return getAllModels().filter((model) => model.provider === provider)
}

// Default temperature value
export function getDefaultTemperature(model: AIModel): number | null {
  return supportsTemperature(model) ? 0.7 : null
}
