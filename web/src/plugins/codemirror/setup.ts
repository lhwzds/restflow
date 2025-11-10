import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view'
import { EditorState, type Extension } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import {
  bracketMatching,
  indentOnInput,
  syntaxHighlighting,
  defaultHighlightStyle,
} from '@codemirror/language'
import { autocompletion, completionKeymap } from '@codemirror/autocomplete'
import { expressionLanguage } from './expressionLang'
import { dragAndDropPlugin } from './dragAndDrop'

/**
 * Base editor extensions for all CodeMirror instances
 */
export const baseExtensions: Extension[] = [
  lineNumbers(),
  highlightActiveLineGutter(),
  history(),
  indentOnInput(),
  bracketMatching(),
  syntaxHighlighting(defaultHighlightStyle),
  EditorView.lineWrapping,
  keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap, indentWithTab]),
]

/**
 * Create expression editor extensions
 * Includes syntax highlighting for {{variable}} syntax and drag & drop support
 */
export function createExpressionExtensions(options?: {
  multiline?: boolean
  autocomplete?: boolean
}): Extension[] {
  const extensions: Extension[] = [...baseExtensions, expressionLanguage, dragAndDropPlugin()]

  // Add autocomplete if enabled
  if (options?.autocomplete) {
    extensions.push(
      autocompletion({
        activateOnTyping: true,
        override: [], // Will be populated by ExpressionInput component
      }),
    )
  }

  // Disable line wrapping for single-line mode
  if (!options?.multiline) {
    extensions.push(
      EditorView.contentAttributes.of({ 'aria-multiline': 'false' }),
      EditorView.domEventHandlers({
        keydown: (event, _view) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault()
            return true
          }
          return false
        },
      }),
    )
  }

  return extensions
}

/**
 * Create a basic CodeMirror editor instance
 */
export function createEditor(
  parent: HTMLElement,
  initialValue: string,
  extensions: Extension[],
  onChange?: (value: string) => void,
): EditorView {
  const startState = EditorState.create({
    doc: initialValue,
    extensions: [
      ...extensions,
      EditorView.updateListener.of((update) => {
        if (update.docChanged && onChange) {
          onChange(update.state.doc.toString())
        }
      }),
    ],
  })

  return new EditorView({
    state: startState,
    parent,
  })
}
