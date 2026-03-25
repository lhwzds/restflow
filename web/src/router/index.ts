import { createRouter, createWebHistory, type RouteLocationRaw } from 'vue-router'
import { BackendError } from '@/api/http-client'
import {
  getExecutionRunThread,
  listExecutionContainers,
  listExecutionSessions,
} from '@/api/execution-console'

function canonicalContainerRoute(containerId: string): RouteLocationRaw {
  return {
    name: 'workspace-container',
    params: { containerId },
  }
}

function canonicalContainerRunRoute(containerId: string, runId: string): RouteLocationRaw {
  return {
    name: 'workspace-container-run',
    params: { containerId, runId },
  }
}

function isNotFoundError(error: unknown): boolean {
  return error instanceof BackendError && error.code === 404
}

export async function resolveLegacySessionRoute(sessionId: string): Promise<RouteLocationRaw> {
  try {
    const containers = await listExecutionContainers()
    const container =
      containers.find((entry) => entry.id === sessionId || entry.latest_session_id === sessionId) ?? null

    if (!container) {
      return canonicalContainerRoute(sessionId)
    }

    if (container.latest_run_id) {
      return canonicalContainerRunRoute(container.id, container.latest_run_id)
    }

    return canonicalContainerRoute(container.id)
  } catch {
    return canonicalContainerRoute(sessionId)
  }
}

export async function resolveLegacyTaskRoute(taskId: string, preferredRunId?: string | null): Promise<RouteLocationRaw> {
  if (preferredRunId) {
    return canonicalContainerRunRoute(taskId, preferredRunId)
  }

  try {
    const runs = await listExecutionSessions({
      container: {
        kind: 'background_task',
        id: taskId,
      },
    })
    const latestRunId = runs.find((entry) => !!entry.run_id)?.run_id ?? null
    if (latestRunId) {
      return canonicalContainerRunRoute(taskId, latestRunId)
    }
  } catch {
    // Fall through to the container route.
  }

  return canonicalContainerRoute(taskId)
}

export async function resolveLegacyRunIdRoute(runId: string): Promise<RouteLocationRaw> {
  try {
    const thread = await getExecutionRunThread(runId)
    return canonicalContainerRunRoute(thread.focus.container_id, thread.focus.run_id ?? runId)
  } catch (error) {
    if (!isNotFoundError(error)) {
      console.warn('Failed to normalize legacy run route:', error)
    }
    return { name: 'workspace' }
  }
}

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
      path: '/workspace/c/:containerId',
      name: 'workspace-container',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/c/:containerId/r/:runId',
      name: 'workspace-container-run',
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/sessions/:sessionId',
      name: 'workspace-session',
      beforeEnter: async (to) => resolveLegacySessionRoute(String(to.params.sessionId ?? '').trim()),
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/runs/:taskId',
      name: 'workspace-run',
      beforeEnter: async (to) =>
        resolveLegacyTaskRoute(
          String(to.params.taskId ?? '').trim(),
          typeof to.query.runId === 'string' ? to.query.runId.trim() : null,
        ),
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
    {
      path: '/workspace/run/:runId',
      name: 'workspace-run-id',
      beforeEnter: async (to) => resolveLegacyRunIdRoute(String(to.params.runId ?? '').trim()),
      component: () => import('../views/Workspace.vue'),
      meta: { titleKey: 'common.brandName' },
    },
  ],
})

export default router
