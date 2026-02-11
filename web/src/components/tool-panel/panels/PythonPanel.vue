<script setup lang="ts">
const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <div class="space-y-2 text-xs">
    <pre class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.code ?? props.data.script ?? '' }}</pre>
    <pre class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.stdout ?? props.data.output ?? props.data.result ?? props.data.raw ?? '' }}</pre>
    <pre v-if="props.data.stderr" class="font-mono bg-red-500/10 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.stderr }}</pre>
  </div>
</template>
