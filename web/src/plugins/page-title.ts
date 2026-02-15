import { watch } from 'vue'

type RouteMeta = Record<string, unknown> | undefined

type RouterLike = {
  currentRoute: {
    value: {
      meta?: RouteMeta
    }
  }
  afterEach: (hook: () => void) => void
}

type I18nLike = {
  global: {
    locale: {
      value: string
    }
    t: (key: string) => string | unknown
  }
}

const DEFAULT_TITLE_KEY = 'common.brandName'
const DEFAULT_BRAND_TITLE = 'RestFlow'

function resolveTitleKey(meta: RouteMeta): string {
  const titleKey = meta?.titleKey
  return typeof titleKey === 'string' && titleKey.trim() !== '' ? titleKey : DEFAULT_TITLE_KEY
}

export function resolveDocumentTitle(router: RouterLike, i18n: I18nLike): string {
  const titleKey = resolveTitleKey(router.currentRoute.value.meta)
  const localizedTitle = i18n.global.t(titleKey)
  return typeof localizedTitle === 'string' && localizedTitle.trim() !== ''
    ? localizedTitle
    : DEFAULT_BRAND_TITLE
}

export function syncDocumentTitle(router: RouterLike, i18n: I18nLike) {
  const applyTitle = () => {
    if (typeof document === 'undefined') {
      return
    }
    document.title = resolveDocumentTitle(router, i18n)
  }

  router.afterEach(() => {
    applyTitle()
  })

  watch(
    () => i18n.global.locale.value,
    () => {
      applyTitle()
    },
  )

  applyTitle()
}
