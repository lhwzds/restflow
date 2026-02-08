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
      meta: { title: 'RestFlow' },
    },
  ],
})

export default router
