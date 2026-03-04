import { createPinia } from 'pinia'
import { createApp } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'

/* Tailwind CSS v4 */
import './styles/tailwind.css'

/* vue-sonner toast styles */
import 'vue-sonner/style.css'

/* Custom theme overrides */
import './styles/theme/index.scss'

import App from './App.vue'
import { isTauri } from './api/tauri-client'
import { preloadPlugin } from './plugins/pinia-preload'
import i18n from './plugins/i18n'
import { syncDocumentTitle } from './plugins/page-title'
import router from './router'
import './style.scss'

async function enableMocking() {
  const isPlaywright =
    typeof navigator !== 'undefined' &&
    (navigator.userAgent.includes('Playwright') || navigator.webdriver)

  // Enable mocking in demo mode or during Playwright E2E runs
  if (import.meta.env.VITE_DEMO_MODE === 'true' || isPlaywright) {
    // Use Tauri IPC mock to intercept invoke() calls directly.
    // MSW only intercepts HTTP requests, but our API layer uses Tauri IPC,
    // so mockIPC is the correct approach.
    const { setupTauriMock } = await import('./mocks/tauri-ipc')
    setupTauriMock()
  }
}

async function applyWindowSpecificRoute() {
  if (!isTauri()) return

  try {
    const label = getCurrentWindow().label
    if (label === 'tray-dashboard' && router.currentRoute.value.path !== '/tray') {
      await router.replace('/tray')
    }
  } catch (error) {
    console.warn('[Bootstrap] Failed to resolve current window label', error)
  }
}

enableMocking().then(async () => {
  const app = createApp(App)
  const pinia = createPinia()

  pinia.use(preloadPlugin)

  app.use(pinia)
  app.use(i18n)
  app.use(router)
  await applyWindowSpecificRoute()
  syncDocumentTitle(router, i18n)
  app.mount('#app')
})
