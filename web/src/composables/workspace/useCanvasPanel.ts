/**
 * Canvas Panel Composable
 *
 * Manages the AI-controlled Canvas panel state. Detects `show_panel`
 * tool calls from the chat stream and renders content in the right panel.
 */

import { ref, computed, watch, type Ref } from 'vue'
import type { StreamState } from './useChatStream'

export interface CanvasState {
  /** Panel title */
  title: string
  /** Content to render */
  content: string
  /** Content type hint */
  contentType: 'markdown' | 'code' | 'json' | 'html'
  /** Whether the panel is visible */
  visible: boolean
}

function createInitialState(): CanvasState {
  return {
    title: '',
    content: '',
    contentType: 'markdown',
    visible: false,
  }
}

/**
 * Composable for the AI-controlled Canvas panel.
 *
 * Watches the chat stream state for `show_panel` tool calls and
 * updates the canvas content accordingly.
 *
 * @param streamState - Reactive ref to the chat stream state
 */
export function useCanvasPanel(streamState: Ref<StreamState>) {
  const state = ref<CanvasState>(createInitialState())

  // Track processed tool call indices to avoid re-processing
  let lastProcessedStepCount = 0

  watch(
    () => streamState.value.steps,
    (steps) => {
      if (steps.length <= lastProcessedStepCount) return

      // Check new steps for show_panel tool calls
      for (let i = lastProcessedStepCount; i < steps.length; i++) {
        const step = steps[i]
        if (
          step &&
          step.type === 'tool_call' &&
          step.name === 'show_panel' &&
          step.status === 'completed'
        ) {
          // Content is handled via handleShowPanelResult() called from parent
        }
      }

      lastProcessedStepCount = steps.length
    },
    { deep: true },
  )

  /**
   * Open the canvas with specific content (called externally or from event handler)
   */
  function openCanvas(title: string, content: string, contentType: string = 'markdown') {
    state.value = {
      title,
      content,
      contentType: contentType as CanvasState['contentType'],
      visible: true,
    }
  }

  /**
   * Close the canvas panel
   */
  function closeCanvas() {
    state.value.visible = false
  }

  /**
   * Clear canvas content and hide
   */
  function clearCanvas() {
    state.value = createInitialState()
    lastProcessedStepCount = 0
  }

  /**
   * Handle a tool_call_end event for show_panel.
   * Called from the parent component when it detects a show_panel tool result.
   */
  function handleShowPanelResult(resultJson: string) {
    try {
      const result = JSON.parse(resultJson)
      if (result.displayed) {
        openCanvas(result.title || '', result.content || '', result.content_type || 'markdown')
      }
    } catch {
      // Ignore parse errors
    }
  }

  const visible = computed(() => state.value.visible)
  const title = computed(() => state.value.title)
  const content = computed(() => state.value.content)
  const contentType = computed(() => state.value.contentType)

  return {
    state,
    visible,
    title,
    content,
    contentType,
    openCanvas,
    closeCanvas,
    clearCanvas,
    handleShowPanelResult,
  }
}

export type UseCanvasPanelReturn = ReturnType<typeof useCanvasPanel>
