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
      path: '/workspace/sessions/:sessionId',
      name: 'workspace-session',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/runs/:taskId',
      name: 'workspace-run',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/run/:runId',
      name: 'workspace-run-id',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
  ],
})

export default router
