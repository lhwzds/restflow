import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/agents',
    },
    {
      path: '/agents',
      name: 'agents',
      component: () => import('../views/AgentManagement.vue'),
      meta: { title: 'Agent Management' },
    },
    {
      path: '/agent/:id',
      name: 'agent-editor',
      component: () => import('../views/AgentEditor.vue'),
      meta: { title: 'Agent Editor' },
    },
    {
      path: '/secrets',
      name: 'secrets',
      component: () => import('../views/SecretManagement.vue'),
      meta: { title: 'Secrets Management' },
    },
    {
      path: '/skills',
      name: 'skills',
      component: () => import('../views/SkillManagement.vue'),
      meta: { title: 'Skill Management' },
    },
    {
      path: '/skill/:id',
      name: 'skill-editor',
      component: () => import('../views/SkillEditor.vue'),
      meta: { title: 'Skill Editor' },
    },
  ],
})

export default router
