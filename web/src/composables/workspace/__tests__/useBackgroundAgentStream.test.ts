import { describe, expect, it } from 'vitest'
import * as legacyStreamModule from '../useBackgroundAgentStream'
import { useTaskStream } from '../useTaskStream'

describe('useBackgroundAgentStream', () => {
  it('aliases the legacy composable name without re-exporting the canonical symbol', () => {
    expect(legacyStreamModule.useBackgroundAgentStream).toBe(useTaskStream)
    expect('useTaskStream' in legacyStreamModule).toBe(false)
  })
})
