import { createPinia } from 'pinia'
import { createApp } from 'vue'

/* Tailwind CSS v4 */
import './styles/tailwind.css'

/* vue-sonner toast styles */
import 'vue-sonner/style.css'

/* Custom theme overrides */
import './styles/theme/index.scss'

import App from './App.vue'
import { preloadPlugin } from './plugins/pinia-preload'
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

enableMocking().then(() => {
  const app = createApp(App)
  const pinia = createPinia()

  pinia.use(preloadPlugin)

  app.use(pinia)
  app.use(router)
  app.mount('#app')
})
