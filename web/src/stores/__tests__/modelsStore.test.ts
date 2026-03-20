import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useModelsStore } from '../modelsStore'

vi.mock('@/api/config', () => ({
  getAvailableModels: vi.fn(),
}))

describe('modelsStore getters', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('returns sorted unique providers', () => {
    const store = useModelsStore()
    store.$patch({
      models: [
        {
          model: 'gpt-5.4',
          provider: 'codex',
          supports_temperature: false,
          name: 'GPT-5.4',
        },
        {
          model: 'claude-code-sonnet',
          provider: 'claude-code',
          supports_temperature: true,
          name: 'Claude Code Sonnet',
        },
        {
          model: 'minimax-coding-plan-m2-5',
          provider: 'minimax-coding-plan',
          supports_temperature: false,
          name: 'MiniMax M2.5 Coding Plan',
        },
        {
          model: 'minimax-coding-plan-m2-5-highspeed',
          provider: 'minimax-coding-plan',
          supports_temperature: false,
          name: 'MiniMax M2.5 Highspeed Coding Plan',
        },
        {
          model: 'glm-5-turbo',
          provider: 'zai-coding-plan',
          supports_temperature: true,
          name: 'GLM-5 Turbo Coding Plan',
        },
        { model: 'gpt-5', provider: 'openai', supports_temperature: false, name: 'GPT-5' },
        {
          model: 'claude-sonnet-4-5',
          provider: 'anthropic',
          supports_temperature: true,
          name: 'Claude Sonnet 4.5',
        },
        { model: 'gpt-5-mini', provider: 'openai', supports_temperature: false, name: 'GPT-5 Mini' },
      ],
    })

    expect(store.getProviders).toEqual([
      'openai',
      'minimax-coding-plan',
      'zai-coding-plan',
      'claude-code',
      'codex',
      'anthropic',
    ])
  })

  it('returns first model for provider and membership checks', () => {
    const store = useModelsStore()
    store.$patch({
      models: [
        { model: 'gpt-5', provider: 'openai', supports_temperature: false, name: 'GPT-5' },
        {
          model: 'claude-sonnet-4-5',
          provider: 'anthropic',
          supports_temperature: true,
          name: 'Claude Sonnet 4.5',
        },
      ],
    })

    expect(store.getFirstModelByProvider('openai')).toBe('gpt-5')
    expect(store.isModelInProvider('openai', 'gpt-5')).toBe(true)
    expect(store.isModelInProvider('openai', 'claude-sonnet-4-5')).toBe(false)
  })
})
