<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  data: Record<string, unknown>
}>()

const formatted = computed(() => JSON.stringify(props.data, null, 2))
</script>

<template>
  <pre class="text-xs font-mono bg-muted/50 rounded-md p-3 overflow-auto whitespace-pre-wrap break-words">{{ formatted }}</pre>
</template>
