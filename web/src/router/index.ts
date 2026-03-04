import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/workspace',
    },
    {
      path: '/workspace',
      name: 'workspace',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/tray',
      name: 'tray-dashboard',
      component: () => import('../views/TrayDashboard.vue'),
      meta: { titleKey: 'common.brandName' },
    },
  ],
})

export default router
