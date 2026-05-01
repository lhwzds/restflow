<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { highlightCode, normalizeLanguage } from '@/utils/codeHighlight'

const props = defineProps<{
  content: string
  inline?: boolean
}>()

const rootRef = ref<HTMLElement | null>(null)
const copyHandlers = new WeakMap<HTMLButtonElement, EventListener>()
let renderVersion = 0

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

function cleanupCopyHandlers() {
  const root = rootRef.value
  if (!root) return

  const buttons = root.querySelectorAll<HTMLButtonElement>('.rf-code-copy-btn')
  for (const button of buttons) {
    const handler = copyHandlers.get(button)
    if (handler) {
      button.removeEventListener('click', handler)
      copyHandlers.delete(button)
    }
  }
}

function extractCodeLanguage(codeElement: HTMLElement): string {
  const languageMatch = codeElement.className.match(/language-([a-z0-9+-]+)/i)
  return normalizeLanguage(languageMatch?.[1] || 'text')
}

function setCopyButtonState(button: HTMLButtonElement, text: string) {
  button.textContent = text
}

function attachCopyHandler(button: HTMLButtonElement, code: string) {
  const handler = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopyButtonState(button, 'Copied')
      window.setTimeout(() => setCopyButtonState(button, 'Copy'), 2000)
    } catch {
      setCopyButtonState(button, 'Failed')
      window.setTimeout(() => setCopyButtonState(button, 'Copy'), 2000)
    }
  }

  button.addEventListener('click', handler)
  copyHandlers.set(button, handler)
}

function createCodeBlockWrapper(code: string, language: string, originalPre: HTMLElement) {
  const wrapper = document.createElement('div')
  wrapper.className = 'rf-code-block'
  wrapper.dataset.lang = language

  const header = document.createElement('div')
  header.className = 'rf-code-header'

  const langBadge = document.createElement('span')
  langBadge.className = 'rf-code-lang'
  langBadge.textContent = language

  const copyButton = document.createElement('button')
  copyButton.type = 'button'
  copyButton.className = 'rf-code-copy-btn'
  copyButton.textContent = 'Copy'
  copyButton.setAttribute('aria-label', 'Copy code')
  attachCopyHandler(copyButton, code)

  header.append(langBadge, copyButton)

  const content = document.createElement('div')
  content.className = 'rf-code-content'
  content.appendChild(originalPre.cloneNode(true))

  wrapper.append(header, content)
  return { wrapper, content }
}

async function enhanceCodeBlocks(version: number) {
  if (props.inline) return

  const root = rootRef.value
  if (!root) return

  cleanupCopyHandlers()
  const codeBlocks = Array.from(root.querySelectorAll<HTMLElement>('pre > code'))

  for (const codeElement of codeBlocks) {
    if (version !== renderVersion) return

    const preElement = codeElement.parentElement as HTMLElement | null
    if (!preElement) continue

    const language = extractCodeLanguage(codeElement)
    const sourceCode = codeElement.textContent || ''
    const { wrapper, content } = createCodeBlockWrapper(sourceCode, language, preElement)
    preElement.replaceWith(wrapper)

    try {
      const highlightedHtml = await highlightCode(sourceCode, language)
      if (version !== renderVersion) return
      content.innerHTML = highlightedHtml
    } catch {
      // Keep existing fallback pre/code content
    }
  }
}

watch(
  html,
  async () => {
    const nextVersion = ++renderVersion
    await nextTick()
    await enhanceCodeBlocks(nextVersion)
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  cleanupCopyHandlers()
})
</script>

<template>
  <div ref="rootRef" class="markdown-renderer" :class="{ inline: inline }" v-html="html" />
</template>

<style lang="scss">
.markdown-renderer {
  font-size: var(--rf-font-size-base);
  line-height: var(--rf-line-height-base);
  color: var(--rf-color-text-regular);
  word-wrap: break-word;

  h1,
  h2,
  h3,
  h4,
  h5,
  h6 {
    margin: var(--rf-spacing-lg) 0 var(--rf-spacing-sm);
    font-weight: var(--rf-font-weight-semibold);
    line-height: var(--rf-line-height-tight);
    color: var(--rf-color-text-primary);

    &:first-child {
      margin-top: 0;
    }
  }

  h1 {
    font-size: var(--rf-font-size-2xl);
  }
  h2 {
    font-size: var(--rf-font-size-xl);
  }
  h3 {
    font-size: var(--rf-font-size-lg);
  }
  h4 {
    font-size: var(--rf-font-size-md);
  }
  h5 {
    font-size: var(--rf-font-size-base);
  }
  h6 {
    font-size: var(--rf-font-size-sm);
  }

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
    padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
    margin: 0 var(--rf-spacing-3xs);
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

  .rf-code-block {
    margin: var(--rf-spacing-md) 0;
    border: 1px solid var(--rf-color-border-light);
    border-radius: var(--rf-radius-base);
    overflow: hidden;
    background: var(--rf-color-bg-secondary);
  }

  .rf-code-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--rf-spacing-sm);
    padding: var(--rf-spacing-xs) var(--rf-spacing-sm);
    border-bottom: 1px solid var(--rf-color-border-light);
    background: color-mix(in srgb, var(--rf-color-bg-secondary) 90%, black 10%);
  }

  .rf-code-lang {
    font-size: var(--rf-font-size-xs);
    text-transform: uppercase;
    color: var(--rf-color-text-secondary);
  }

  .rf-code-copy-btn {
    border: none;
    background: transparent;
    color: var(--rf-color-text-secondary);
    font-size: var(--rf-font-size-xs);
    cursor: pointer;
    padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
    border-radius: var(--rf-radius-small);
  }

  .rf-code-copy-btn:hover {
    background: color-mix(in srgb, var(--rf-color-primary) 12%, transparent);
    color: var(--rf-color-text-primary);
  }

  .rf-code-content {
    overflow-x: auto;
  }

  .rf-code-fallback {
    margin: 0;
    padding: var(--rf-spacing-md);
  }

  :deep(.shiki) {
    margin: 0;
    padding: var(--rf-spacing-md);
    overflow-x: auto;
    background: var(--rf-color-bg-secondary);
  }

  :deep(.shiki code) {
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: var(--rf-font-size-sm);
    line-height: var(--rf-line-height-base);
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
      th,
      tr:nth-child(even) {
        background: var(--rf-md-block-bg);
      }
    }
  }
}
</style>
