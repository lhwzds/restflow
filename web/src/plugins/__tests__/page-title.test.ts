import { ref } from 'vue'
import { describe, expect, it, vi } from 'vitest'
import { resolveDocumentTitle, syncDocumentTitle } from '../page-title'

function createMockRouter(titleKey?: string) {
  const routeRef = ref<{ meta?: Record<string, unknown> }>({
    meta: titleKey ? { titleKey } : {},
  })
  const hooks: Array<() => void> = []

  return {
    currentRoute: routeRef,
    afterEach(hook: () => void) {
      hooks.push(hook)
    },
    triggerAfterEach() {
      hooks.forEach((hook) => hook())
    },
  }
}

function createMockI18n(initialLocale = 'zh-CN') {
  const locale = ref(initialLocale)
  const messages: Record<string, Record<string, string>> = {
    'zh-CN': { 'common.brandName': '浮流 RestFlow' },
    en: { 'common.brandName': 'RestFlow' },
  }

  return {
    global: {
      locale,
      t: (key: string) => messages[locale.value]?.[key] ?? '',
    },
  }
}

describe('page title plugin', () => {
  it('resolves route title by titleKey translation', () => {
    const router = createMockRouter('common.brandName')
    const i18n = createMockI18n()

    expect(resolveDocumentTitle(router, i18n)).toBe('浮流 RestFlow')
  })

  it('falls back to default brand title when translation is empty', () => {
    const router = createMockRouter('unknown.title')
    const i18n = createMockI18n()

    expect(resolveDocumentTitle(router, i18n)).toBe('RestFlow')
  })

  it('syncs document title on locale and route updates', async () => {
    const router = createMockRouter('common.brandName')
    const i18n = createMockI18n()

    syncDocumentTitle(router, i18n)
    expect(document.title).toBe('浮流 RestFlow')

    i18n.global.locale.value = 'en'
    await vi.waitFor(() => {
      expect(document.title).toBe('RestFlow')
    })

    router.currentRoute.value.meta = {}
    router.triggerAfterEach()
    expect(document.title).toBe('RestFlow')
  })
})
