import { describe, expect, it } from 'vitest'
import { getProviderDisplayName, sortProviders } from '../providerCatalog'

describe('providerCatalog', () => {
  it('sorts primary providers in product order before other providers', () => {
    expect(
      sortProviders([
        'anthropic',
        'codex',
        'openai',
        'zai-coding-plan',
        'minimax-coding-plan',
      ]),
    ).toEqual([
      'openai',
      'minimax-coding-plan',
      'zai-coding-plan',
      'codex',
      'anthropic',
    ])
  })

  it('returns stable display labels for provider groups', () => {
    expect(getProviderDisplayName('openai')).toBe('OpenAI API')
    expect(getProviderDisplayName('minimax-coding-plan')).toBe('MiniMax Coding Plan')
    expect(getProviderDisplayName('zai-coding-plan')).toBe('ZAI Coding Plan')
    expect(getProviderDisplayName('codex')).toBe('Codex')
  })
})
