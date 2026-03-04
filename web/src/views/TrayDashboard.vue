<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { Window, getCurrentWindow } from '@tauri-apps/api/window'
import { Activity, Bot, Clock3, RefreshCcw } from 'lucide-vue-next'
import AgentStatusBadge from '@/components/background-agent/AgentStatusBadge.vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { isTauri } from '@/api/tauri-client'
import { useToast } from '@/composables/useToast'
import { useTrayDashboardMetrics } from '@/composables/tray/useTrayDashboardMetrics'

const toast = useToast()
const { agents, error, isLoading, isRefreshing, lastUpdatedAt, metrics, refresh } =
  useTrayDashboardMetrics()

const REFRESH_INTERVAL_MS = 15_000
let refreshTimer: number | null = null

const hasTrendData = computed(() => metrics.value.trend.some((bucket) => bucket.runs > 0))
const maxTrendTokens = computed(() =>
  Math.max(...metrics.value.trend.map((bucket) => bucket.tokens), 1),
)

function formatCompactNumber(value: number): string {
  return new Intl.NumberFormat('en-US', { notation: 'compact' }).format(value)
}

function formatUsd(value: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 2,
  }).format(value)
}

function formatDuration(value: number | null): string {
  if (value == null || value <= 0) return 'N/A'
  if (value < 1000) return `${value}ms`
  if (value < 60_000) return `${(value / 1000).toFixed(1)}s`
  return `${(value / 60_000).toFixed(1)}m`
}

function formatSuccessRate(rate: number | null): string {
  if (rate == null) return 'N/A'
  return `${(rate * 100).toFixed(1)}%`
}

function formatTimestamp(value: number | null): string {
  if (value == null) return 'N/A'
  return new Date(value).toLocaleString()
}

function trendBarHeight(tokens: number): string {
  const normalized = Math.max(0.08, tokens / maxTrendTokens.value)
  return `${Math.round(normalized * 100)}%`
}

async function openMainWorkspace(): Promise<void> {
  if (!isTauri()) return

  try {
    const mainWindow = await Window.getByLabel('main')
    if (!mainWindow) return

    await mainWindow.show()
    await mainWindow.unminimize()
    await mainWindow.setFocus()

    const currentWindow = getCurrentWindow()
    if (currentWindow.label === 'tray-dashboard') {
      await currentWindow.hide()
    }
  } catch (openError) {
    console.warn('[TrayDashboard] Failed to open main window', openError)
    toast.error('Unable to open main window. Use tray menu: Open Main Window.')
  }
}

async function refreshNow(): Promise<void> {
  await refresh()
}

onMounted(async () => {
  await refresh()

  refreshTimer = window.setInterval(() => {
    void refresh()
  }, REFRESH_INTERVAL_MS)
})

onUnmounted(() => {
  if (refreshTimer != null) {
    window.clearInterval(refreshTimer)
    refreshTimer = null
  }
})
</script>

<template>
  <div
    data-testid="tray-dashboard-root"
    class="min-h-screen bg-background px-3 py-3 text-foreground"
  >
    <div class="mx-auto flex max-w-[36rem] flex-col gap-3">
      <Card>
        <CardHeader class="pb-3">
          <div class="flex items-start justify-between gap-3">
            <div class="space-y-1">
              <CardTitle class="text-base">Mini Dashboard</CardTitle>
              <p class="text-xs text-muted-foreground">
                Running {{ metrics.kpis.runningAgents }} / {{ metrics.kpis.totalAgents }} agents
              </p>
            </div>
            <div class="flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                class="h-8 px-2"
                :disabled="isRefreshing"
                @click="refreshNow"
              >
                <RefreshCcw class="mr-1 h-3.5 w-3.5" />
                Refresh
              </Button>
              <Button size="sm" class="h-8 px-2" @click="openMainWorkspace">Open Main</Button>
            </div>
          </div>
          <p class="text-xs text-muted-foreground">
            Last update: {{ formatTimestamp(lastUpdatedAt) }}
          </p>
        </CardHeader>
      </Card>

      <div
        v-if="error"
        class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs"
      >
        {{ error }}
      </div>

      <div
        v-if="isLoading"
        class="rounded-md border border-border bg-card px-3 py-4 text-sm text-muted-foreground"
      >
        Loading mini dashboard...
      </div>

      <template v-else>
        <div class="grid grid-cols-2 gap-3">
          <Card>
            <CardContent class="p-4">
              <div class="mb-2 flex items-center gap-2 text-muted-foreground">
                <Bot class="h-4 w-4" />
                <span class="text-xs">Running</span>
              </div>
              <div data-testid="tray-kpi-running" class="text-xl font-semibold">
                {{ metrics.kpis.runningAgents }}
              </div>
              <div class="text-xs text-muted-foreground">
                Active: {{ metrics.kpis.activeAgents }}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent class="p-4">
              <div class="mb-2 flex items-center gap-2 text-muted-foreground">
                <Activity class="h-4 w-4" />
                <span class="text-xs">Success Rate</span>
              </div>
              <div data-testid="tray-kpi-success-rate" class="text-xl font-semibold">
                {{ formatSuccessRate(metrics.kpis.successRate) }}
              </div>
              <div class="text-xs text-muted-foreground">Runs: {{ metrics.kpis.totalRuns }}</div>
            </CardContent>
          </Card>

          <Card>
            <CardContent class="p-4">
              <div class="mb-2 flex items-center gap-2 text-muted-foreground">
                <Clock3 class="h-4 w-4" />
                <span class="text-xs">Avg Duration</span>
              </div>
              <div data-testid="tray-kpi-duration" class="text-xl font-semibold">
                {{ formatDuration(metrics.kpis.avgDurationMs) }}
              </div>
              <div class="text-xs text-muted-foreground">
                Last run: {{ formatTimestamp(metrics.kpis.lastRunAt) }}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent class="p-4">
              <div class="mb-2 flex items-center gap-2 text-muted-foreground">
                <Activity class="h-4 w-4" />
                <span class="text-xs">Usage</span>
              </div>
              <div data-testid="tray-kpi-cost" class="text-xl font-semibold">
                {{ formatUsd(metrics.kpis.totalCostUsd) }}
              </div>
              <div class="text-xs text-muted-foreground">
                Tokens: {{ formatCompactNumber(metrics.kpis.totalTokens) }}
              </div>
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardHeader class="pb-2">
            <CardTitle class="text-sm">Usage Trend (Recent)</CardTitle>
          </CardHeader>
          <CardContent class="pt-0">
            <div
              v-if="!hasTrendData"
              class="rounded-md border border-dashed px-3 py-4 text-xs text-muted-foreground"
            >
              No execution events yet. Usage trend appears after background runs finish.
            </div>
            <div v-else data-testid="tray-trend-chart" class="flex h-24 items-end gap-1">
              <div
                v-for="bucket in metrics.trend"
                :key="bucket.startAt"
                class="group relative flex-1 rounded-sm bg-primary/20"
                :title="`${new Date(bucket.startAt).toLocaleTimeString()} · runs ${bucket.runs} · tokens ${bucket.tokens}`"
              >
                <div
                  class="absolute inset-x-0 bottom-0 rounded-sm bg-primary transition-all duration-300"
                  :style="{ height: trendBarHeight(bucket.tokens) }"
                />
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader class="pb-2">
            <CardTitle class="text-sm">Model Usage (Estimated by Agent Model)</CardTitle>
          </CardHeader>
          <CardContent class="pt-0">
            <div
              v-if="metrics.modelUsage.length === 0"
              class="rounded-md border border-dashed px-3 py-4 text-xs text-muted-foreground"
            >
              No model usage data yet.
            </div>
            <div v-else data-testid="tray-model-list" class="space-y-2">
              <div
                v-for="modelItem in metrics.modelUsage"
                :key="modelItem.model"
                class="grid grid-cols-[1fr_auto_auto] items-center gap-2 rounded-md border px-3 py-2"
              >
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium">{{ modelItem.model }}</p>
                  <p class="text-xs text-muted-foreground">
                    {{ modelItem.agentCount }} agents · {{ modelItem.runningCount }} running
                  </p>
                </div>
                <span class="text-xs text-muted-foreground">
                  {{ formatCompactNumber(modelItem.tokens) }} tok
                </span>
                <span class="text-xs font-medium">{{ formatUsd(modelItem.costUsd) }}</span>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader class="pb-2">
            <CardTitle class="text-sm">Agent Status</CardTitle>
          </CardHeader>
          <CardContent class="pt-0">
            <div
              v-if="agents.length === 0"
              class="rounded-md border border-dashed px-3 py-4 text-xs text-muted-foreground"
            >
              No background agents found.
            </div>
            <div v-else data-testid="tray-agent-list" class="space-y-2">
              <div
                v-for="agent in metrics.topAgents"
                :key="agent.id"
                class="flex items-center justify-between rounded-md border px-3 py-2"
              >
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium">{{ agent.name }}</p>
                  <p class="text-xs text-muted-foreground">
                    {{ formatTimestamp(agent.updatedAt) }}
                  </p>
                </div>
                <AgentStatusBadge :status="agent.status" />
              </div>
            </div>
          </CardContent>
        </Card>
      </template>
    </div>
  </div>
</template>
