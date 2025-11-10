/**
 * Model Configuration Constants
 * Provides centralized model management with internal IDs and user-friendly display names
 */

export interface ModelOption {
  value: string
  label: string
  provider: 'openai' | 'anthropic' | 'deepseek'
}

/**
 * Available model options for dropdowns
 * - value: Internal model ID (stored in backend)
 * - label: User-friendly display name (shown in UI)
 */
export const MODEL_OPTIONS: ModelOption[] = [
  // OpenAI Models
  { value: 'gpt-5', label: 'GPT-5', provider: 'openai' },
  { value: 'gpt-4.1', label: 'GPT-4.1', provider: 'openai' },
  { value: 'gpt-4.1-mini', label: 'GPT-4.1 Mini', provider: 'openai' },
  { value: 'gpt-4.1-nano', label: 'GPT-4.1 Nano', provider: 'openai' },
  { value: 'gpt-4o', label: 'GPT-4o', provider: 'openai' },
  { value: 'gpt-4o-mini', label: 'GPT-4o Mini', provider: 'openai' },
  { value: 'gpt-4', label: 'GPT-4', provider: 'openai' },
  { value: 'gpt-4-turbo', label: 'GPT-4 Turbo', provider: 'openai' },
  { value: 'gpt-3.5-turbo', label: 'GPT-3.5 Turbo', provider: 'openai' },
  { value: 'o4-mini', label: 'O4 Mini', provider: 'openai' },
  { value: 'o3', label: 'O3', provider: 'openai' },
  { value: 'o3-mini', label: 'O3 Mini', provider: 'openai' },

  // Anthropic Models (rig format: claude-{version}-{model})
  { value: 'claude-4-sonnet', label: 'Claude 4 Sonnet', provider: 'anthropic' },
  { value: 'claude-4-opus', label: 'Claude 4 Opus', provider: 'anthropic' },
  { value: 'claude-3.7-sonnet', label: 'Claude 3.7 Sonnet', provider: 'anthropic' },

  // DeepSeek Models
  { value: 'deepseek-chat', label: 'DeepSeek Chat', provider: 'deepseek' },
  { value: 'deepseek-reasoner', label: 'DeepSeek Reasoner', provider: 'deepseek' },
]

/**
 * Model display name mapping
 * Maps internal model ID to user-friendly display name
 */
export const MODEL_DISPLAY_NAMES: Record<string, string> = Object.fromEntries(
  MODEL_OPTIONS.map((option) => [option.value, option.label]),
)

/**
 * Get user-friendly display name for a model
 * @param model - Internal model ID
 * @returns User-friendly display name or original model ID if not found
 */
export function getModelDisplayName(model?: string): string {
  if (!model) return 'Unknown Model'
  return MODEL_DISPLAY_NAMES[model] || model
}

/**
 * Get tag type (color) for a model based on provider
 * @param model - Internal model ID
 * @returns Element Plus tag type
 */
export function getModelTagType(
  model?: string,
): 'success' | 'primary' | 'warning' | 'info' | 'danger' {
  if (!model) return 'info'

  const option = MODEL_OPTIONS.find((opt) => opt.value === model)
  if (!option) {
    // Fallback to string matching for backward compatibility
    if (model.includes('gpt')) return 'success'
    if (model.includes('claude')) return 'warning'
    if (model.includes('deepseek')) return 'primary'
    return 'info'
  }

  switch (option.provider) {
    case 'openai':
      return 'success'
    case 'anthropic':
      return 'warning'
    case 'deepseek':
      return 'primary'
    default:
      return 'info'
  }
}

/**
 * Get models by provider
 * @param provider - AI provider name
 * @returns Array of model options for the specified provider
 */
export function getModelsByProvider(provider: 'openai' | 'anthropic' | 'deepseek'): ModelOption[] {
  return MODEL_OPTIONS.filter((opt) => opt.provider === provider)
}
