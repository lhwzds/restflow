import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import {
  getProvider,
  supportsTemperature,
  getModelDisplayName,
  getDefaultTemperature,
  getAllModels,
  getModelsByProvider,
  getProviderTagType,
} from '../AIModels'
import { useModelsStore } from '@/stores/modelsStore'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'

// Mock the config module that modelsStore imports (tauriInvoke)
vi.mock('@/api/config', () => ({
  tauriInvoke: vi.fn(),
}))

const MOCK_MODELS: ModelMetadataDTO[] = [
  { model: 'gpt-5', provider: 'openai', supports_temperature: true, name: 'GPT-5' },
  {
    model: 'claude-sonnet-4-5',
    provider: 'anthropic',
    supports_temperature: true,
    name: 'Claude Sonnet 4.5',
  },
  {
    model: 'deepseek-chat',
    provider: 'deepseek',
    supports_temperature: false,
    name: 'DeepSeek Chat',
  },
  {
    model: 'gemini-2-5-pro',
    provider: 'google',
    supports_temperature: true,
    name: 'Gemini 2.5 Pro',
  },
]

describe('AIModels utility', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // Pre-populate the models store so utility functions can resolve metadata
    const store = useModelsStore()
    store.models = [...MOCK_MODELS]
    store.loaded = true
  })

  // ---------------------------------------------------------------------------
  // getProvider
  // ---------------------------------------------------------------------------

  describe('getProvider', () => {
    it('returns the correct provider for a known model', () => {
      expect(getProvider('gpt-5')).toBe('openai')
      expect(getProvider('claude-sonnet-4-5')).toBe('anthropic')
      expect(getProvider('deepseek-chat')).toBe('deepseek')
    })

    it('falls back to openai for an unknown model', () => {
      expect(getProvider('nonexistent-model' as any)).toBe('openai')
    })
  })

  // ---------------------------------------------------------------------------
  // supportsTemperature
  // ---------------------------------------------------------------------------

  describe('supportsTemperature', () => {
    it('returns true for models that support temperature', () => {
      expect(supportsTemperature('gpt-5')).toBe(true)
      expect(supportsTemperature('claude-sonnet-4-5')).toBe(true)
    })

    it('returns false for models that do not support temperature', () => {
      expect(supportsTemperature('deepseek-chat')).toBe(false)
    })

    it('returns false for unknown models', () => {
      expect(supportsTemperature('nonexistent' as any)).toBe(false)
    })
  })

  // ---------------------------------------------------------------------------
  // getModelDisplayName
  // ---------------------------------------------------------------------------

  describe('getModelDisplayName', () => {
    it('returns the display name from metadata', () => {
      expect(getModelDisplayName('gpt-5')).toBe('GPT-5')
      expect(getModelDisplayName('claude-sonnet-4-5')).toBe('Claude Sonnet 4.5')
    })

    it('falls back to the model ID for unknown models', () => {
      const unknownModel = 'totally-unknown' as any
      expect(getModelDisplayName(unknownModel)).toBe(unknownModel)
    })
  })

  // ---------------------------------------------------------------------------
  // getDefaultTemperature
  // ---------------------------------------------------------------------------

  describe('getDefaultTemperature', () => {
    it('returns 0.7 for models that support temperature', () => {
      expect(getDefaultTemperature('gpt-5')).toBe(0.7)
    })

    it('returns undefined for models that do not support temperature', () => {
      expect(getDefaultTemperature('deepseek-chat')).toBeUndefined()
    })
  })

  // ---------------------------------------------------------------------------
  // getAllModels
  // ---------------------------------------------------------------------------

  describe('getAllModels', () => {
    it('returns all models as ModelOption objects', () => {
      const result = getAllModels()

      expect(result).toHaveLength(MOCK_MODELS.length)
      expect(result[0]).toEqual({
        value: 'gpt-5',
        label: 'GPT-5',
        provider: 'openai',
        supportsTemperature: true,
      })
    })

    it('returns an empty array when the store has no models', () => {
      const store = useModelsStore()
      store.models = []

      expect(getAllModels()).toEqual([])
    })
  })

  // ---------------------------------------------------------------------------
  // getModelsByProvider
  // ---------------------------------------------------------------------------

  describe('getModelsByProvider', () => {
    it('returns only models for the specified provider', () => {
      const result = getModelsByProvider('anthropic')

      expect(result).toHaveLength(1)
      expect(result[0]!.value).toBe('claude-sonnet-4-5')
      expect(result[0]!.provider).toBe('anthropic')
    })

    it('returns empty array for a provider with no models', () => {
      expect(getModelsByProvider('groq')).toEqual([])
    })
  })

  // ---------------------------------------------------------------------------
  // getProviderTagType
  // ---------------------------------------------------------------------------

  describe('getProviderTagType', () => {
    it('returns success for openai', () => {
      expect(getProviderTagType('openai')).toBe('success')
    })

    it('returns warning for anthropic', () => {
      expect(getProviderTagType('anthropic')).toBe('warning')
    })

    it('returns info for deepseek', () => {
      expect(getProviderTagType('deepseek')).toBe('info')
    })

    it('returns info for other providers', () => {
      expect(getProviderTagType('google')).toBe('info')
      expect(getProviderTagType('groq')).toBe('info')
    })
  })
})
