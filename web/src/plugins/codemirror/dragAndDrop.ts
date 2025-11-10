import { ViewPlugin, EditorView, type ViewUpdate } from '@codemirror/view'
import { StateField, StateEffect, type Extension } from '@codemirror/state'
import { useDragAndDrop } from '@/composables/ui/useDragAndDrop'

/**
 * State field to track drop cursor position
 */
const dropCursorPos = StateField.define<number | null>({
  create() {
    return null
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setDropCursor)) {
        return effect.value
      }
    }
    return value
  },
})

const setDropCursor = StateEffect.define<number | null>()

/**
 * CodeMirror plugin that integrates with global drag & drop system
 * Allows inserting dragged variables into the editor
 */
export const dragAndDropPlugin = (): Extension => {
  const { isDragging, dragData, endDrag } = useDragAndDrop()

  return [
    dropCursorPos,
    ViewPlugin.fromClass(
      class {
        dropPos: number | null = null
        view: EditorView

        constructor(view: EditorView) {
          this.view = view
          // Add global mouseup listener to handle drop
          document.addEventListener('mouseup', this.handleGlobalMouseUp)
        }

        update(update: ViewUpdate) {
          // Update drop cursor if position changed
          const pos = update.state.field(dropCursorPos)
          if (pos !== this.dropPos) {
            this.dropPos = pos
          }
        }

        handleGlobalMouseUp = (e: MouseEvent) => {
          if (!isDragging.value || !dragData.value) return

          // Check if drop target is this editor
          const editorElement = this.view.dom
          const rect = editorElement.getBoundingClientRect()
          const isInEditor =
            e.clientX >= rect.left &&
            e.clientX <= rect.right &&
            e.clientY >= rect.top &&
            e.clientY <= rect.bottom

          if (isInEditor && this.dropPos !== null) {
            // Insert dragged data at cursor position
            const data = dragData.value.data
            this.view.dispatch({
              changes: {
                from: this.dropPos,
                to: this.dropPos,
                insert: data,
              },
              selection: { anchor: this.dropPos + data.length },
            })
          }

          // Clear drop cursor
          this.view.dispatch({
            effects: setDropCursor.of(null),
          })

          endDrag()
        }

        destroy() {
          document.removeEventListener('mouseup', this.handleGlobalMouseUp)
        }
      },
      {
        eventHandlers: {
          mousemove(event, view) {
            if (!isDragging.value) return

            // Calculate drop position
            const pos = view.posAtCoords({ x: event.clientX, y: event.clientY })
            if (pos !== null) {
              view.dispatch({
                effects: setDropCursor.of(pos),
              })
            }
          },

          mouseleave(_event, view) {
            // Clear drop cursor when leaving editor
            view.dispatch({
              effects: setDropCursor.of(null),
            })
          },
        },
      },
    ),
  ]
}

export { dropCursorPos }
