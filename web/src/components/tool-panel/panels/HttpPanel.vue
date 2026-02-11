<script setup lang="ts">
const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <div class="space-y-2 text-xs">
    <div><span class="text-muted-foreground">Method:</span> {{ props.data.method ?? 'N/A' }}</div>
    <div><span class="text-muted-foreground">URL:</span> {{ props.data.url ?? 'N/A' }}</div>
    <div><span class="text-muted-foreground">Status:</span> {{ props.data.status ?? props.data.status_code ?? 'N/A' }}</div>
    <pre class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.body ?? props.data.response ?? props.data.raw ?? '' }}</pre>
  </div>
</template>
