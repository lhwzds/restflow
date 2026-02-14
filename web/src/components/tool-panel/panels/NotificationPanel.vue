<script setup lang="ts">
const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <div class="space-y-2 text-xs">
    <div>
      <span class="text-muted-foreground">Target:</span>
      {{ props.data.to ?? props.data.chat_id ?? 'N/A' }}
    </div>
    <div><span class="text-muted-foreground">Status:</span> {{ props.data.status ?? 'sent' }}</div>
    <pre
      class="font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words"
      >{{ props.data.message ?? props.data.body ?? props.data.raw ?? '' }}</pre
    >
  </div>
</template>
