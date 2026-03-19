import type { Provider } from '@/types/generated/Provider'

const PRIMARY_PROVIDER_ORDER: Provider[] = [
  'openai',
  'minimax-coding-plan',
  'zai-coding-plan',
  'claude-code',
  'codex',
]

const SECONDARY_PROVIDER_ORDER: Provider[] = [
  'anthropic',
  'google',
  'deepseek',
  'groq',
  'openrouter',
  'xai',
  'qwen',
  'zai',
  'moonshot',
  'doubao',
  'yi',
  'siliconflow',
  'minimax',
]

const PROVIDER_ORDER: Provider[] = [...PRIMARY_PROVIDER_ORDER, ...SECONDARY_PROVIDER_ORDER]

const PROVIDER_LABELS: Record<Provider, string> = {
  openai: 'OpenAI API',
  anthropic: 'Anthropic API',
  'claude-code': 'Claude Code',
  codex: 'Codex',
  deepseek: 'DeepSeek',
  google: 'Google',
  groq: 'Groq',
  openrouter: 'OpenRouter',
  xai: 'XAI',
  qwen: 'Qwen',
  zai: 'ZAI',
  'zai-coding-plan': 'ZAI Coding Plan',
  moonshot: 'Moonshot',
  doubao: 'Doubao',
  yi: 'Yi',
  siliconflow: 'SiliconFlow',
  minimax: 'MiniMax',
  'minimax-coding-plan': 'MiniMax Coding Plan',
}

export function sortProviders(providers: Provider[]): Provider[] {
  return [...providers].sort((left, right) => {
    const leftIndex = PROVIDER_ORDER.indexOf(left)
    const rightIndex = PROVIDER_ORDER.indexOf(right)
    const leftOrder = leftIndex === -1 ? Number.MAX_SAFE_INTEGER : leftIndex
    const rightOrder = rightIndex === -1 ? Number.MAX_SAFE_INTEGER : rightIndex
    return leftOrder - rightOrder || left.localeCompare(right)
  })
}

export function getProviderDisplayName(provider: Provider): string {
  return PROVIDER_LABELS[provider] ?? provider
}
