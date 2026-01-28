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
      component: () => import('../views/SkillWorkspace.vue'),
      meta: { title: 'RestFlow' },
    },
    {
      path: '/agent-tasks',
      name: 'agent-tasks',
      component: () => import('../views/AgentTaskList.vue'),
      meta: { title: 'Agent Tasks - RestFlow' },
    },
  ],
})

export default router
