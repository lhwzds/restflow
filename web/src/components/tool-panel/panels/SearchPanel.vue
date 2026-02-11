<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  data: Record<string, unknown>
}>()

const items = computed(() => {
  if (Array.isArray(props.data.results)) return props.data.results
  return []
})
</script>

<template>
  <div class="space-y-2 text-xs">
    <div v-if="items.length === 0" class="text-muted-foreground">No structured results, showing raw payload.</div>
    <div v-for="(item, idx) in items" :key="idx" class="border border-border rounded-md p-2">
      <div class="font-medium">{{ (item as any).title ?? 'Untitled' }}</div>
      <div class="text-muted-foreground break-all">{{ (item as any).url ?? '' }}</div>
      <div>{{ (item as any).snippet ?? '' }}</div>
    </div>
    <pre v-if="items.length === 0" class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ props.data.raw ?? JSON.stringify(props.data, null, 2) }}</pre>
  </div>
</template>
