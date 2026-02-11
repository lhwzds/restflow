<script setup lang="ts">
const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <div class="space-y-2">
    <div class="text-xs text-muted-foreground">
      Exit Code: {{ props.data.exit_code ?? props.data.code ?? 'N/A' }}
    </div>
    <pre class="text-xs font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.stdout ?? props.data.output ?? props.data.raw ?? '' }}</pre>
    <pre v-if="props.data.stderr" class="text-xs font-mono bg-red-500/10 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.stderr }}</pre>
  </div>
</template>
