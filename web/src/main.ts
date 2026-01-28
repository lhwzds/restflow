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
  // Enable MSW only in demo mode
  if (import.meta.env.VITE_DEMO_MODE === 'true') {
    const { worker } = await import('./mocks/browser')

    return worker.start({
      onUnhandledRequest: 'bypass',
    })
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
