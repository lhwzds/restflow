import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import { createApp } from 'vue'
import App from './App.vue'
import './style.css'

createApp(App)

const app = createApp(App)
app.use(ElementPlus)
app.mount('#app')
