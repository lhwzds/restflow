import ElementPlus from 'element-plus'
import { createPinia } from 'pinia'
import { createApp } from 'vue'

import '@vue-flow/controls/dist/style.css'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/minimap/dist/style.css'

import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'

/* Custom theme overrides */
import './styles/theme/index.scss'

import App from './App.vue'
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

  app.use(pinia)
  app.use(router)
  app.use(ElementPlus)
  app.mount('#app')
})
