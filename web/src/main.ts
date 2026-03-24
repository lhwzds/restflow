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
import i18n from './plugins/i18n'
import { syncDocumentTitle } from './plugins/page-title'
import router from './router'
import './style.scss'

const app = createApp(App)
const pinia = createPinia()

pinia.use(preloadPlugin)

app.use(pinia)
app.use(i18n)
app.use(router)
syncDocumentTitle(router, i18n)
app.mount('#app')
