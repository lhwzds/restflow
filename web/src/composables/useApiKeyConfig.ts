import type { ApiKeyConfig } from '@/types/generated/ApiKeyConfig'

/**
 * Composable for managing API key configuration
 * Provides utilities to build, extract, and compare ApiKeyConfig objects
 */
export function useApiKeyConfig() {
  /**
   * Build an ApiKeyConfig object from mode and value
   */
  const buildConfig = (mode: 'direct' | 'secret', value: string): ApiKeyConfig | undefined => {
    if (!value || !value.trim()) {
      return undefined
    }

    return {
      type: mode,
      value: value.trim(),
    } as ApiKeyConfig
  }

  /**
   * Extract mode and value from ApiKeyConfig
   */
  const extractConfig = (
    config: ApiKeyConfig | undefined,
  ): { mode: 'direct' | 'secret'; value: string } => {
    if (!config) {
      return { mode: 'direct', value: '' }
    }

    return {
      mode: config.type,
      value: config.value,
    }
  }

  /**
   * Check if two ApiKeyConfig objects are different
   */
  const isConfigChanged = (
    oldConfig: ApiKeyConfig | undefined,
    newConfig: ApiKeyConfig | undefined,
  ): boolean => {
    // Both null
    if (!oldConfig && !newConfig) {
      return false
    }

    // One is null
    if (!oldConfig || !newConfig) {
      return true
    }

    // Compare type and value
    return oldConfig.type !== newConfig.type || oldConfig.value !== newConfig.value
  }

  /**
   * Get display text for API key config
   */
  const getConfigDisplay = (config: ApiKeyConfig | undefined): string => {
    if (!config) {
      return 'Not configured'
    }

    if (config.type === 'direct') {
      return `Direct (${config.value.substring(0, 8)}...)`
    }

    return `Secret: ${config.value}`
  }

  return {
    buildConfig,
    extractConfig,
    isConfigChanged,
    getConfigDisplay,
  }
}
