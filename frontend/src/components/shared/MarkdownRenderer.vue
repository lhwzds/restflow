<script setup lang="ts">
import { computed } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const props = defineProps<{
  content: string
  inline?: boolean
}>()

marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    link(token) {
      const href = token.href || ''
      const title = token.title ? ` title="${token.title}"` : ''
      const text = token.text || ''
      return `<a href="${href}" target="_blank" rel="noopener noreferrer"${title}>${text}</a>`
    },
  },
})

const html = computed(() => {
  if (!props.content) return ''

  const rawHtml = props.inline ? marked.parseInline(props.content) : marked.parse(props.content)

  return DOMPurify.sanitize(rawHtml as string, {
    ADD_ATTR: ['target', 'rel'],
    FORCE_BODY: true,
  })
})
</script>

<template>
  <div class="markdown-renderer" :class="{ inline: inline }" v-html="html" />
</template>

<style lang="scss">
.markdown-renderer {
  font-size: var(--rf-font-size-base);
  line-height: var(--rf-line-height-base);
  color: var(--rf-color-text-regular);
  word-wrap: break-word;

  h1, h2, h3, h4, h5, h6 {
    margin: var(--rf-spacing-lg) 0 var(--rf-spacing-sm);
    font-weight: var(--rf-font-weight-semibold);
    line-height: var(--rf-line-height-tight);
    color: var(--rf-color-text-primary);

    &:first-child {
      margin-top: 0;
    }
  }

  h1 { font-size: var(--rf-font-size-2xl); }
  h2 { font-size: var(--rf-font-size-xl); }
  h3 { font-size: var(--rf-font-size-lg); }
  h4 { font-size: var(--rf-font-size-md); }
  h5 { font-size: var(--rf-font-size-base); }
  h6 { font-size: var(--rf-font-size-sm); }

  p {
    margin: var(--rf-spacing-sm) 0;

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
    margin: var(--rf-spacing-sm) 0;
    padding-left: var(--rf-spacing-2xl);

    li {
      margin: var(--rf-spacing-xs) 0;
    }

    ul,
    ol {
      margin: var(--rf-spacing-xs) 0;
    }
  }

  code {
    padding: var(--rf-spacing-2xs) var(--rf-spacing-xs);
    margin: 0 var(--rf-spacing-2xs);
    background: var(--rf-color-bg-secondary);
    border-radius: var(--rf-radius-small);
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-danger);
  }

  pre {
    margin: var(--rf-spacing-md) 0;
    padding: var(--rf-spacing-md);
    background: var(--rf-color-bg-secondary);
    border-radius: var(--rf-radius-base);
    overflow-x: auto;

    code {
      padding: 0;
      margin: 0;
      background: transparent;
      color: var(--rf-color-text-regular);
      font-size: var(--rf-font-size-sm);
      line-height: var(--rf-line-height-base);
    }
  }

  blockquote {
    margin: var(--rf-spacing-md) 0;
    padding: var(--rf-spacing-sm) var(--rf-spacing-lg);
    border-left: var(--rf-spacing-xs) solid var(--rf-color-primary);
    background: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-secondary);

    p {
      margin: var(--rf-spacing-xs) 0;
    }
  }

  table {
    width: 100%;
    margin: var(--rf-spacing-md) 0;
    border-collapse: collapse;

    th,
    td {
      padding: var(--rf-spacing-sm) var(--rf-spacing-md);
      border: 1px solid var(--rf-color-border-light);
    }

    th {
      background: var(--rf-color-bg-secondary);
      font-weight: var(--rf-font-weight-semibold);
      text-align: left;
      color: var(--rf-color-text-primary);
    }

    tr:nth-child(even) {
      background: var(--rf-color-bg-secondary);
    }
  }

  hr {
    margin: var(--rf-spacing-lg) 0;
    border: none;
    border-top: 1px solid var(--rf-color-border-light);
  }

  img {
    max-width: 100%;
    height: auto;
    border-radius: var(--rf-radius-small);
  }

  strong {
    font-weight: var(--rf-font-weight-semibold);
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

html.dark {
  .markdown-renderer {
    --rf-md-code-bg: rgba(255, 255, 255, 0.1);
    --rf-md-block-bg: rgba(255, 255, 255, 0.05);
    --rf-md-code-color: var(--rf-color-primary);

    code {
      background: var(--rf-md-code-bg);
      color: var(--rf-md-code-color);
    }

    pre {
      background: var(--rf-md-block-bg);

      code {
        background: transparent;
        color: var(--rf-color-text-regular);
      }
    }

    blockquote {
      background: var(--rf-md-block-bg);
    }

    table {
      th, tr:nth-child(even) {
        background: var(--rf-md-block-bg);
      }
    }
  }
}
</style>
