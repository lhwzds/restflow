import { StreamLanguage } from '@codemirror/language'

/**
 * Simple expression language for highlighting {{variable}} syntax
 * This is a lightweight implementation that only highlights interpolation
 * without full JavaScript expression support
 */
const expressionLanguage = StreamLanguage.define({
  name: 'restflow-expression',

  token(stream, state: any) {
    // Check for opening braces {{
    if (stream.match('{{')) {
      state.inBraces = true
      return 'bracket'
    }

    // Check for closing braces }}
    if (state.inBraces && stream.match('}}')) {
      state.inBraces = false
      return 'bracket'
    }

    // Inside braces: highlight variable paths
    if (state.inBraces) {
      // Match variable paths like trigger.payload, node.http1.body
      if (stream.match(/[a-zA-Z_][a-zA-Z0-9_.]*/)) {
        return 'variableName'
      }

      // Match array indices [0]
      if (stream.match(/\[\d+\]/)) {
        return 'number'
      }

      stream.next()
      return null
    }

    // Outside braces: plain text
    // Fast path: skip to next {{ or end of line
    const nextBrace = stream.string.indexOf('{{', stream.pos)
    if (nextBrace > stream.pos) {
      stream.pos = nextBrace
    } else {
      stream.skipToEnd()
    }

    return null
  },

  startState() {
    return {
      inBraces: false,
    }
  },
})

export { expressionLanguage }

/**
 * Tag styles for expression language
 */
export const expressionHighlightStyle = {
  '.cm-bracket': { color: '#0288d1', fontWeight: 'bold' },
  '.cm-variableName': { color: '#6f42c1' },
  '.cm-number': { color: '#d73a49' },
}
