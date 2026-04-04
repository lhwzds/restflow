import { describe, expect, it } from 'vitest'

import * as composables from '../index'

describe('composables index', () => {
  it('only exposes canonical task stream exports from the aggregate surface', () => {
    expect(composables.useTaskStream).toBeTypeOf('function')
    expect('useBackgroundAgentStream' in composables).toBe(false)
  })
})
