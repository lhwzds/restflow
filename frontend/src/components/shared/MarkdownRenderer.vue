<script setup lang="ts">
import { computed } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const props = defineProps<{
  content: string
  inline?: boolean
}>()

// Configure marked with minimal options
marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    link(href, title, text) {
      return `<a href="${href}" target="_blank" rel="noopener noreferrer"${title ? ` title="${title}"` : ''}>${text}</a>`
    },
  },
})

// Parse and sanitize markdown (simplified)
const html = computed(() => {
  if (!props.content) return ''

  const rawHtml = props.inline ? marked.parseInline(props.content) : marked.parse(props.content)

  // Use DOMPurify defaults (they're already comprehensive)
  // Only add target and rel for links to open in new tab
  return DOMPurify.sanitize(rawHtml as string, {
    ADD_ATTR: ['target', 'rel'],
    ADD_TAGS: ['#text'],
    FORCE_BODY: true,
  })
})
</script>

<template>
  <div class="markdown-renderer" :class="{ inline: inline }" v-html="html" />
</template>

<style lang="scss">
.markdown-renderer {
  font-size: 14px;
  line-height: 1.6;
  color: var(--rf-color-text-regular);
  word-wrap: break-word;

  h1,
  h2,
  h3,
  h4,
  h5,
  h6 {
    margin: 16px 0 8px;
    font-weight: 600;
    line-height: 1.25;
    color: var(--rf-color-text-primary);

    &:first-child {
      margin-top: 0;
    }
  }

  h1 {
    font-size: 24px;
  }
  h2 {
    font-size: 20px;
  }
  h3 {
    font-size: 18px;
  }
  h4 {
    font-size: 16px;
  }
  h5 {
    font-size: 14px;
  }
  h6 {
    font-size: 12px;
  }

  p {
    margin: 8px 0;

    &:first-child {
      margin-top: 0;
    }

    &:last-child {
      margin-bottom: 0;
    }
  }

  a {
    color: var(--rf-color-primary);
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }

  ul,
  ol {
    margin: 8px 0;
    padding-left: 24px;

    li {
      margin: 4px 0;
    }

    ul,
    ol {
      margin: 4px 0;
    }
  }

  code {
    padding: 2px 6px;
    margin: 0 2px;
    background: var(--rf-color-bg-secondary);
    border-radius: 3px;
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: 13px;
    color: var(--rf-color-danger);
  }

  pre {
    margin: 12px 0;
    padding: 12px;
    background: var(--rf-color-bg-secondary);
    border-radius: 6px;
    overflow-x: auto;

    code {
      padding: 0;
      margin: 0;
      background: transparent;
      color: var(--rf-color-text-regular);
      font-size: 13px;
      line-height: 1.5;
    }
  }

  blockquote {
    margin: 12px 0;
    padding: 8px 16px;
    border-left: 4px solid var(--rf-color-primary);
    background: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-secondary);

    p {
      margin: 4px 0;
    }
  }

  table {
    width: 100%;
    margin: 12px 0;
    border-collapse: collapse;
    overflow: auto;

    th,
    td {
      padding: 8px 12px;
      border: 1px solid var(--rf-color-border-light);
    }

    th {
      background: var(--rf-color-bg-secondary);
      font-weight: 600;
      text-align: left;
      color: var(--rf-color-text-primary);
    }

    tr:nth-child(even) {
      background: var(--rf-color-bg-secondary);
    }
  }

  hr {
    margin: 16px 0;
    border: none;
    border-top: 1px solid var(--rf-color-border-light);
  }

  img {
    max-width: 100%;
    height: auto;
    border-radius: 4px;
  }

  strong {
    font-weight: 600;
    color: var(--rf-color-text-primary);
  }

  em {
    font-style: italic;
  }

  // Inline mode
  &.inline {
    display: inline;

    p {
      display: inline;
      margin: 0;
    }
  }
}

// Dark theme
html.dark {
  .markdown-renderer {
    code {
      background: rgba(255, 255, 255, 0.1);
      color: #ff79c6;
    }

    pre {
      background: rgba(0, 0, 0, 0.3);

      code {
        color: var(--rf-color-text-regular);
      }
    }

    blockquote {
      background: rgba(255, 255, 255, 0.05);
      border-left-color: var(--rf-color-primary);
    }

    table {
      th {
        background: rgba(255, 255, 255, 0.05);
      }

      tr:nth-child(even) {
        background: rgba(255, 255, 255, 0.03);
      }
    }
  }
}
</style>
