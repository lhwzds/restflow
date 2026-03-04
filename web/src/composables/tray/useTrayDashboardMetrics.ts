import { computed, ref } from 'vue'
import { listAgents } from '@/api/agents'
import { getBackgroundAgentEvents, listBackgroundAgents } from '@/api/background-agents'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { BackgroundAgentStatus } from '@/types/generated/BackgroundAgentStatus'
import type { TaskEvent } from '@/types/generated/TaskEvent'

const DEFAULT_BUCKET_SIZE_MS = 15 * 60 * 1000
const DEFAULT_BUCKET_COUNT = 16
const EVENTS_FETCH_LIMIT = 200

export interface UsageTrendBucket {
  startAt: number
  tokens: number
  costUsd: number
  durationMs: number
  runs: number
}

export interface ModelUsageSummary {
  model: string
  agentCount: number
  runningCount: number
  tokens: number
  costUsd: number
}

export interface AgentSnapshot {
  id: string
  name: string
  status: BackgroundAgentStatus
  updatedAt: number
  totalTokens: number
  totalCostUsd: number
}

export interface TrayKpiMetrics {
  totalAgents: number
  runningAgents: number
  activeAgents: number
  pausedAgents: number
  completedAgents: number
  failedAgents: number
  totalRuns: number
  successRate: number | null
  totalTokens: number
  totalCostUsd: number
  avgDurationMs: number | null
  lastRunAt: number | null
}

export interface TrayDashboardMetrics {
  kpis: TrayKpiMetrics
  trend: UsageTrendBucket[]
  modelUsage: ModelUsageSummary[]
  topAgents: AgentSnapshot[]
  lastEventAt: number | null
}

interface BuildTrayDashboardMetricsInput {
  agents: BackgroundAgent[]
  eventsByTask: Record<string, TaskEvent[]>
  modelByAgentId: Map<string, string>
  now?: number
  bucketSizeMs?: number
  bucketCount?: number
}

function normalizeModel(model: string | null | undefined): string {
  if (!model || model.trim().length === 0) return 'auto'
  return model.trim()
}

function extractMetricEvents(eventsByTask: Record<string, TaskEvent[]>): TaskEvent[] {
  return Object.values(eventsByTask)
    .flat()
    .filter(
      (event) =>
        event.tokens_used != null ||
        event.cost_usd != null ||
        event.duration_ms != null ||
        event.event_type === 'completed' ||
        event.event_type === 'failed' ||
        event.event_type === 'interrupted',
    )
}

function buildTrendBuckets(
  events: TaskEvent[],
  now: number,
  bucketSizeMs: number,
  bucketCount: number,
): UsageTrendBucket[] {
  const latestTimestamp = events.reduce((max, event) => Math.max(max, event.timestamp), 0)
  const anchor = latestTimestamp > 0 ? latestTimestamp : now
  const lastBucketStart = Math.floor(anchor / bucketSizeMs) * bucketSizeMs
  const firstBucketStart = lastBucketStart - (bucketCount - 1) * bucketSizeMs

  const buckets: UsageTrendBucket[] = Array.from({ length: bucketCount }, (_, index) => ({
    startAt: firstBucketStart + index * bucketSizeMs,
    tokens: 0,
    costUsd: 0,
    durationMs: 0,
    runs: 0,
  }))

  for (const event of events) {
    if (event.timestamp < firstBucketStart || event.timestamp > lastBucketStart + bucketSizeMs) {
      continue
    }

    const index = Math.floor((event.timestamp - firstBucketStart) / bucketSizeMs)
    const bucket = buckets[index]
    if (!bucket) continue

    bucket.tokens += event.tokens_used ?? 0
    bucket.costUsd += event.cost_usd ?? 0
    bucket.durationMs += event.duration_ms ?? 0
    bucket.runs += 1
  }

  return buckets
}

function statusPriority(status: BackgroundAgentStatus): number {
  switch (status) {
    case 'running':
      return 0
    case 'active':
      return 1
    case 'failed':
      return 2
    case 'paused':
      return 3
    case 'interrupted':
      return 4
    case 'completed':
      return 5
    default:
      return 6
  }
}

export function buildTrayDashboardMetrics(
  input: BuildTrayDashboardMetricsInput,
): TrayDashboardMetrics {
  const {
    agents,
    eventsByTask,
    modelByAgentId,
    now = Date.now(),
    bucketSizeMs = DEFAULT_BUCKET_SIZE_MS,
    bucketCount = DEFAULT_BUCKET_COUNT,
  } = input

  const metricEvents = extractMetricEvents(eventsByTask)
  const trend = buildTrendBuckets(metricEvents, now, bucketSizeMs, bucketCount)

  let successCount = 0
  let failureCount = 0
  let totalTokens = 0
  let totalCostUsd = 0
  let lastRunAt: number | null = null

  const statusCounter: Record<BackgroundAgentStatus, number> = {
    active: 0,
    paused: 0,
    running: 0,
    completed: 0,
    failed: 0,
    interrupted: 0,
  }

  const modelMap = new Map<string, ModelUsageSummary>()
  for (const agent of agents) {
    successCount += agent.success_count
    failureCount += agent.failure_count
    totalTokens += agent.total_tokens_used
    totalCostUsd += agent.total_cost_usd

    if (agent.last_run_at != null) {
      lastRunAt = lastRunAt == null ? agent.last_run_at : Math.max(lastRunAt, agent.last_run_at)
    }

    statusCounter[agent.status] += 1

    const model = normalizeModel(modelByAgentId.get(agent.agent_id))
    const entry = modelMap.get(model) ?? {
      model,
      agentCount: 0,
      runningCount: 0,
      tokens: 0,
      costUsd: 0,
    }
    entry.agentCount += 1
    entry.runningCount += agent.status === 'running' ? 1 : 0
    entry.tokens += agent.total_tokens_used
    entry.costUsd += agent.total_cost_usd
    modelMap.set(model, entry)
  }

  const durationEvents = metricEvents.filter((event) => event.duration_ms != null)
  const totalDurationMs = durationEvents.reduce((sum, event) => sum + (event.duration_ms ?? 0), 0)
  const avgDurationMs =
    durationEvents.length > 0 ? Math.round(totalDurationMs / durationEvents.length) : null

  const totalRuns = successCount + failureCount
  const successRate = totalRuns > 0 ? successCount / totalRuns : null
  const lastEventAt =
    metricEvents.length > 0
      ? metricEvents.reduce((max, event) => Math.max(max, event.timestamp), 0)
      : null

  const modelUsage = Array.from(modelMap.values()).sort((a, b) => {
    if (b.costUsd !== a.costUsd) return b.costUsd - a.costUsd
    if (b.tokens !== a.tokens) return b.tokens - a.tokens
    return a.model.localeCompare(b.model)
  })

  const topAgents = agents
    .map<AgentSnapshot>((agent) => ({
      id: agent.id,
      name: agent.name,
      status: agent.status,
      updatedAt: agent.updated_at,
      totalTokens: agent.total_tokens_used,
      totalCostUsd: agent.total_cost_usd,
    }))
    .sort((a, b) => {
      const priorityDiff = statusPriority(a.status) - statusPriority(b.status)
      if (priorityDiff !== 0) return priorityDiff
      return b.updatedAt - a.updatedAt
    })
    .slice(0, 8)

  return {
    kpis: {
      totalAgents: agents.length,
      runningAgents: statusCounter.running,
      activeAgents: statusCounter.active,
      pausedAgents: statusCounter.paused,
      completedAgents: statusCounter.completed,
      failedAgents: statusCounter.failed,
      totalRuns,
      successRate,
      totalTokens,
      totalCostUsd,
      avgDurationMs,
      lastRunAt,
    },
    trend,
    modelUsage,
    topAgents,
    lastEventAt,
  }
}

export function useTrayDashboardMetrics() {
  const agents = ref<BackgroundAgent[]>([])
  const eventsByTask = ref<Record<string, TaskEvent[]>>({})
  const modelByAgentId = ref<Map<string, string>>(new Map())

  const isLoading = ref(false)
  const isRefreshing = ref(false)
  const error = ref<string | null>(null)
  const lastUpdatedAt = ref<number | null>(null)

  const metrics = computed(() =>
    buildTrayDashboardMetrics({
      agents: agents.value,
      eventsByTask: eventsByTask.value,
      modelByAgentId: modelByAgentId.value,
    }),
  )

  async function refresh(): Promise<void> {
    if (isLoading.value || isRefreshing.value) return

    if (lastUpdatedAt.value == null) {
      isLoading.value = true
    } else {
      isRefreshing.value = true
    }

    try {
      const [nextAgents, storedAgents] = await Promise.all([listBackgroundAgents(), listAgents()])
      const nextModelByAgentId = new Map(
        storedAgents.map((agent) => [agent.id, normalizeModel(agent.agent.model)]),
      )

      const eventEntries = await Promise.all(
        nextAgents.map(async (agent) => {
          try {
            const events = await getBackgroundAgentEvents(agent.id, EVENTS_FETCH_LIMIT)
            return [agent.id, events] as const
          } catch (eventError) {
            console.warn('[TrayDashboard] Failed to fetch events', {
              taskId: agent.id,
              error: eventError,
            })
            return [agent.id, []] as const
          }
        }),
      )

      agents.value = nextAgents
      modelByAgentId.value = nextModelByAgentId
      eventsByTask.value = Object.fromEntries(eventEntries)
      error.value = null
      lastUpdatedAt.value = Date.now()
    } catch (refreshError) {
      error.value =
        refreshError instanceof Error ? refreshError.message : 'Failed to load tray metrics'
    } finally {
      isLoading.value = false
      isRefreshing.value = false
    }
  }

  return {
    agents,
    error,
    isLoading,
    isRefreshing,
    lastUpdatedAt,
    metrics,
    refresh,
  }
}
