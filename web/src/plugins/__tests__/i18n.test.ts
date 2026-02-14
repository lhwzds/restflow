import { beforeEach, describe, expect, it, vi } from 'vitest'

async function loadI18nModule() {
  vi.resetModules()
  return import('../i18n')
}

describe('i18n plugin', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('uses zh-CN as default locale when no stored value exists', async () => {
    const { default: i18n } = await loadI18nModule()
    expect(i18n.global.locale.value).toBe('zh-CN')
  })

  it('uses saved locale from localStorage when available', async () => {
    vi.spyOn(localStorage, 'getItem').mockImplementation((key: string) =>
      key === 'locale' ? 'en' : null,
    )

    const { default: i18n } = await loadI18nModule()
    expect(i18n.global.locale.value).toBe('en')
  })

  it('setLocale updates runtime locale and persists to localStorage', async () => {
    const setItemSpy = vi.spyOn(localStorage, 'setItem')
    const { default: i18n, setLocale } = await loadI18nModule()

    setLocale('en')

    expect(i18n.global.locale.value).toBe('en')
    expect(setItemSpy).toHaveBeenCalledWith('locale', 'en')
  })
})
