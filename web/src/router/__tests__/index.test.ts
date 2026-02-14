import { describe, expect, it } from 'vitest'
import router from '@/router'

describe('router branding', () => {
  it('uses 浮流 RestFlow as workspace page title', () => {
    const workspaceRoute = router.getRoutes().find((route) => route.name === 'workspace')
    expect(workspaceRoute?.meta?.title).toBe('浮流 RestFlow')
  })
})
