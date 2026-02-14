import { createI18n } from 'vue-i18n'
import en from '@/locales/en.json'
import zhCN from '@/locales/zh-CN.json'

export const supportedLocales = ['zh-CN', 'en'] as const
export type SupportedLocale = (typeof supportedLocales)[number]

const DEFAULT_LOCALE: SupportedLocale = 'zh-CN'

function isSupportedLocale(value: string | null): value is SupportedLocale {
  return value !== null && supportedLocales.includes(value as SupportedLocale)
}

function detectInitialLocale(): SupportedLocale {
  if (typeof globalThis.localStorage === 'undefined') {
    return DEFAULT_LOCALE
  }

  const savedLocale = localStorage.getItem('locale')
  if (isSupportedLocale(savedLocale)) {
    return savedLocale
  }

  return DEFAULT_LOCALE
}

const i18n = createI18n({
  legacy: false,
  locale: detectInitialLocale(),
  fallbackLocale: 'en',
  messages: {
    en,
    'zh-CN': zhCN,
  },
})

export function setLocale(locale: SupportedLocale) {
  i18n.global.locale.value = locale

  if (typeof globalThis.localStorage !== 'undefined') {
    localStorage.setItem('locale', locale)
  }
}

export default i18n
