<script setup lang="ts">
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'

const props = defineProps<{
  data: Record<string, unknown>
}>()
</script>

<template>
  <MarkdownRenderer :content="String(props.data.content ?? props.data.text ?? props.data.raw ?? '')" />
</template>
