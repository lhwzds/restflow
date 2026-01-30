<script setup lang="ts">
/**
 * StreamingMarkdown Component
 *
 * Renders markdown content with streaming support.
 * Handles incomplete syntax during streaming and shows a typing indicator.
 */
import { computed, ref, watch } from 'vue'
import MarkdownRenderer from './MarkdownRenderer.vue'

const props = defineProps<{
  /** Markdown content to render */
  content: string
  /** Whether content is still streaming */
  isStreaming?: boolean
  /** Show typing indicator when streaming */
  showCursor?: boolean
}>()

/**
 * Fix incomplete markdown syntax during streaming
 * This prevents rendering errors when content is cut off mid-syntax
 */
function fixIncompleteMarkdown(content: string): string {
  if (!content) return ''

  let result = content

  // Close unclosed code blocks
  const codeBlockMatches = result.match(/```/g) || []
  if (codeBlockMatches.length % 2 !== 0) {
    result += '\n```'
  }

  // Close unclosed inline code
  const inlineCodeMatches = result.match(/(?<!`)`(?!`)/g) || []
  if (inlineCodeMatches.length % 2 !== 0) {
    result += '`'
  }

  // Close unclosed bold
  const boldMatches = result.match(/\*\*/g) || []
  if (boldMatches.length % 2 !== 0) {
    result += '**'
  }

  // Close unclosed italic (single asterisk not followed by another)
  const italicMatches = result.match(/(?<!\*)\*(?!\*)/g) || []
  if (italicMatches.length % 2 !== 0) {
    result += '*'
  }

  // Close unclosed links
  if (result.includes('[') && !result.includes('](')) {
    const lastBracket = result.lastIndexOf('[')
    const textAfter = result.slice(lastBracket)
    if (!textAfter.includes(']')) {
      result += '](#)'
    }
  }

  return result
}

/**
 * Processed content with fixed incomplete syntax
 */
const processedContent = computed(() => {
  if (props.isStreaming) {
    return fixIncompleteMarkdown(props.content)
  }
  return props.content
})

/**
 * Whether to show the typing cursor
 */
const showTypingCursor = computed(() => {
  return props.isStreaming && props.showCursor !== false
})
</script>

<template>
  <div class="streaming-markdown" :class="{ streaming: isStreaming }">
    <MarkdownRenderer :content="processedContent" />
    <span v-if="showTypingCursor" class="typing-cursor" aria-hidden="true">▌</span>
  </div>
</template>

<style lang="scss">
.streaming-markdown {
  position: relative;

  .typing-cursor {
    display: inline-block;
    animation: blink 1s step-end infinite;
    color: var(--rf-color-primary);
    font-weight: normal;
    vertical-align: baseline;
    margin-left: 2px;
  }

  &.streaming {
    .markdown-renderer {
      // Ensure inline display for cursor to follow text
      > *:last-child {
        display: inline;
      }
    }
  }
}

@keyframes blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0;
  }
}
</style>
