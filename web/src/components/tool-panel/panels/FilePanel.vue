<script setup lang="ts">
const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <div class="space-y-2 text-xs">
    <div><span class="text-muted-foreground">Action:</span> {{ props.data.action ?? 'N/A' }}</div>
    <div><span class="text-muted-foreground">Path:</span> {{ props.data.path ?? 'N/A' }}</div>
    <pre class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.content ?? props.data.result ?? props.data.raw ?? '' }}</pre>
  </div>
</template>
