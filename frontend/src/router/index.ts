import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/workflows',
    },
    {
      path: '/workflows',
      name: 'workflows',
      component: () => import('../views/WorkflowList.vue'),
      meta: { title: 'Workflows' },
    },
    {
      path: '/workflow/:id?',
      name: 'workflow-editor',
      component: () => import('../views/WorkflowEditor.vue'),
      meta: { title: 'Editor' },
    },
    {
      path: '/agents',
      name: 'agents',
      component: () => import('../views/AgentManagement.vue'),
      meta: { title: 'Agent Management' },
    },
  ],
})

export default router