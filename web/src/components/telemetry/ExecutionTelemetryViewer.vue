<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useExecutionTelemetry } from '@/composables/telemetry/useExecutionTelemetry'
import type { ExecutionTraceEvent } from '@/types/generated/ExecutionTraceEvent'

const props = defineProps<{
  runId?: string | null
  hideTimeline?: boolean
}>()

const { t } = useI18n()
const activeTab = ref(props.hideTimeline ? 'metrics' : 'timeline')
const runIdRef = computed(() => props.runId ?? null)
const {
  timeline,
  metrics,
  logs,
  isLoadingTimeline,
  isLoadingMetrics,
  isLoadingLogs,
  timelineError,
  metricsError,
  logsError,
} = useExecutionTelemetry(runIdRef)

const stats = computed(() => timeline.value?.stats ?? null)
const timelineEvents = computed(() => timeline.value?.events ?? [])
const metricEvents = computed(() => metrics.value?.samples ?? [])
const logEvents = computed(() => logs.value?.events ?? [])

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp).toLocaleString()
}

function formatCount(value: bigint | number | null | undefined): string {
  if (typeof value === 'bigint') {
    return value.toString()
  }
  if (typeof value === 'number') {
    return Number.isFinite(value) ? value.toLocaleString() : '0'
  }
  return '0'
}

function formatOptionalNumber(value: number | null | undefined, suffix = ''): string | null {
  if (value === null || value === undefined) {
    return null
  }
  return `${value.toLocaleString()}${suffix}`
}

function formatOptionalBigInt(value: bigint | number | null | undefined, suffix = ''): string | null {
  if (value === null || value === undefined) {
    return null
  }
  return `${formatCount(value)}${suffix}`
}

function eventTitle(event: ExecutionTraceEvent): string {
  switch (event.category) {
    case 'lifecycle':
      return event.lifecycle?.status ?? 'lifecycle'
    case 'llm_call':
      return event.llm_call?.model ? `LLM call · ${event.llm_call.model}` : 'LLM call'
    case 'tool_call':
      return event.tool_call
        ? `${event.tool_call.phase === 'started' ? 'Tool start' : 'Tool done'} · ${event.tool_call.tool_name}`
        : 'Tool call'
    case 'model_switch':
      return event.model_switch
        ? `${event.model_switch.from_model} → ${event.model_switch.to_model}`
        : 'Model switch'
    case 'message':
      return event.message?.role ? `Message · ${event.message.role}` : 'Message'
    case 'metric_sample':
      return event.metric_sample?.name ?? 'Metric sample'
    case 'provider_health':
      return event.provider_health?.provider ?? 'Provider health'
    case 'log_record':
      return event.log_record?.level ? `Log · ${event.log_record.level}` : 'Log record'
    default:
      return event.category
  }
}

function eventSummary(event: ExecutionTraceEvent): string | null {
  switch (event.category) {
    case 'lifecycle':
      return event.lifecycle?.message ?? event.lifecycle?.error ?? null
    case 'llm_call':
      return [
        formatOptionalNumber(event.llm_call?.total_tokens, ' tokens'),
        formatOptionalBigInt(event.llm_call?.duration_ms, ' ms'),
        event.llm_call?.cost_usd !== null && event.llm_call?.cost_usd !== undefined
          ? `$${event.llm_call.cost_usd.toFixed(4)}`
          : null,
      ]
        .filter(Boolean)
        .join(' · ')
    case 'tool_call':
      return event.tool_call?.error ?? event.tool_call?.input_summary ?? event.tool_call?.output_ref ?? null
    case 'model_switch':
      return event.model_switch?.reason ?? null
    case 'message':
      return event.message?.content_preview ?? null
    case 'metric_sample':
      return event.metric_sample
        ? `${event.metric_sample.value}${event.metric_sample.unit ? ` ${event.metric_sample.unit}` : ''}`
        : null
    case 'provider_health':
      return event.provider_health
        ? `${event.provider_health.status}${event.provider_health.reason ? ` · ${event.provider_health.reason}` : ''}`
        : null
    case 'log_record':
      return event.log_record?.message ?? null
    default:
      return null
  }
}
</script>

<template>
  <section class="bg-muted/20" data-testid="execution-telemetry-viewer">
    <div class="flex items-center justify-between gap-3 px-4 py-3">
      <div class="min-w-0">
        <h3 class="text-sm font-medium text-foreground">
          {{ t('backgroundAgent.runTraceTitle') }}
        </h3>
        <p class="text-xs text-muted-foreground">
          {{ t('backgroundAgent.runTraceDescription') }}
        </p>
      </div>
      <div v-if="stats" class="flex flex-wrap justify-end gap-2 text-[11px] text-muted-foreground">
        <span class="rounded-full border border-border px-2 py-1">
          {{ t('backgroundAgent.statsEvents', { count: formatCount(stats.total_events) }) }}
        </span>
        <span class="rounded-full border border-border px-2 py-1">
          {{ t('backgroundAgent.statsModels', { count: formatCount(stats.model_switch_count) }) }}
        </span>
        <span class="rounded-full border border-border px-2 py-1">
          {{ t('backgroundAgent.statsTools', { count: formatCount(stats.tool_call_count) }) }}
        </span>
        <span class="rounded-full border border-border px-2 py-1">
          {{ t('backgroundAgent.statsTokens', { count: formatCount(stats.total_tokens) }) }}
        </span>
      </div>
    </div>

    <div class="px-4 pb-4">
      <Tabs v-model:model-value="activeTab" :default-value="hideTimeline ? 'metrics' : 'timeline'">
        <TabsList class="w-full justify-start">
          <TabsTrigger
            v-if="!hideTimeline"
            value="timeline"
            data-testid="execution-telemetry-tab-timeline"
          >
            {{ t('backgroundAgent.timelineTab') }}
          </TabsTrigger>
          <TabsTrigger value="metrics" data-testid="execution-telemetry-tab-metrics">
            {{ t('backgroundAgent.metricsTab') }}
          </TabsTrigger>
          <TabsTrigger value="logs" data-testid="execution-telemetry-tab-logs">
            {{ t('backgroundAgent.logsTab') }}
          </TabsTrigger>
        </TabsList>

        <TabsContent v-if="!hideTimeline" value="timeline" class="mt-3">
          <div v-if="isLoadingTimeline" class="text-sm text-muted-foreground">
            {{ t('backgroundAgent.loadingRun') }}
          </div>
          <div
            v-else-if="timelineError"
            class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive"
          >
            {{ t('backgroundAgent.timelineError') }}: {{ timelineError }}
          </div>
          <div
            v-else-if="timelineEvents.length === 0"
            class="rounded-md border border-dashed border-border px-3 py-6 text-sm text-muted-foreground"
            data-testid="execution-telemetry-empty"
          >
            {{ t('backgroundAgent.timelineEmpty') }}
          </div>
          <div v-else class="space-y-2" data-testid="execution-telemetry-timeline-list">
            <article
              v-for="event in timelineEvents"
              :key="event.id"
              class="rounded-md border border-border bg-background px-3 py-2"
              data-testid="execution-telemetry-event"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="text-sm font-medium text-foreground">{{ eventTitle(event) }}</div>
                  <div v-if="eventSummary(event)" class="mt-1 text-xs text-muted-foreground break-words">
                    {{ eventSummary(event) }}
                  </div>
                  <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                    <span class="rounded-full bg-muted px-2 py-0.5">{{ event.category }}</span>
                    <span v-if="event.attempt !== null" class="rounded-full bg-muted px-2 py-0.5">
                      attempt {{ event.attempt }}
                    </span>
                    <span
                      v-if="event.effective_model"
                      class="rounded-full bg-muted px-2 py-0.5 font-mono"
                    >
                      {{ event.effective_model }}
                    </span>
                  </div>
                </div>
                <time class="shrink-0 text-[11px] text-muted-foreground">
                  {{ formatTimestamp(event.timestamp) }}
                </time>
              </div>
            </article>
          </div>
        </TabsContent>

        <TabsContent value="metrics" class="mt-3">
          <div v-if="isLoadingMetrics" class="text-sm text-muted-foreground">
            {{ t('backgroundAgent.loadingRun') }}
          </div>
          <div
            v-else-if="metricsError"
            class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive"
          >
            {{ t('backgroundAgent.metricsError') }}: {{ metricsError }}
          </div>
          <div
            v-else-if="metricEvents.length === 0"
            class="rounded-md border border-dashed border-border px-3 py-6 text-sm text-muted-foreground"
          >
            {{ t('backgroundAgent.metricsEmpty') }}
          </div>
          <div v-else class="space-y-2" data-testid="execution-telemetry-metrics-list">
            <article
              v-for="event in metricEvents"
              :key="event.id"
              class="rounded-md border border-border bg-background px-3 py-2"
              data-testid="execution-telemetry-metric"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="text-sm font-medium text-foreground">
                    {{ event.metric_sample?.name ?? eventTitle(event) }}
                  </div>
                  <div class="mt-1 text-xs text-muted-foreground">
                    {{ eventSummary(event) }}
                  </div>
                </div>
                <time class="shrink-0 text-[11px] text-muted-foreground">
                  {{ formatTimestamp(event.timestamp) }}
                </time>
              </div>
            </article>
          </div>
        </TabsContent>

        <TabsContent value="logs" class="mt-3">
          <div v-if="isLoadingLogs" class="text-sm text-muted-foreground">
            {{ t('backgroundAgent.loadingRun') }}
          </div>
          <div
            v-else-if="logsError"
            class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive"
          >
            {{ t('backgroundAgent.logsError') }}: {{ logsError }}
          </div>
          <div
            v-else-if="logEvents.length === 0"
            class="rounded-md border border-dashed border-border px-3 py-6 text-sm text-muted-foreground"
          >
            {{ t('backgroundAgent.logsEmpty') }}
          </div>
          <div v-else class="space-y-2" data-testid="execution-telemetry-logs-list">
            <article
              v-for="event in logEvents"
              :key="event.id"
              class="rounded-md border border-border bg-background px-3 py-2"
              data-testid="execution-telemetry-log"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="text-sm font-medium text-foreground">
                    {{ event.log_record?.level ?? 'log' }}
                  </div>
                  <div class="mt-1 text-xs text-muted-foreground break-words">
                    {{ event.log_record?.message }}
                  </div>
                </div>
                <time class="shrink-0 text-[11px] text-muted-foreground">
                  {{ formatTimestamp(event.timestamp) }}
                </time>
              </div>
            </article>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  </section>
</template>
