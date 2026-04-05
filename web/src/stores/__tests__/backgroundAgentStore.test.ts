import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import * as legacyStoreModule from '../backgroundAgentStore'
import { useTaskStore } from '../taskStore'

describe('backgroundAgentStore compat shim', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('aliases useBackgroundAgentStore to the canonical task store factory', () => {
    const canonicalStore = useTaskStore()
    const legacyStore = legacyStoreModule.useBackgroundAgentStore()

    expect(legacyStore).toBe(canonicalStore)
  })

  it('does not add a runtime legacy facade on top of the canonical task store', () => {
    const legacyStore = legacyStoreModule.useBackgroundAgentStore()

    expect('filteredAgents' in legacyStore).toBe(false)
    expect('selectedAgent' in legacyStore).toBe(false)
    expect('runningCount' in legacyStore).toBe(false)
    expect('fetchAgents' in legacyStore).toBe(false)
    expect('pauseAgent' in legacyStore).toBe(false)
    expect('resumeAgent' in legacyStore).toBe(false)
    expect('stopAgent' in legacyStore).toBe(false)
    expect('runAgentNow' in legacyStore).toBe(false)
    expect('deleteAgent' in legacyStore).toBe(false)
    expect('convertSessionToAgent' in legacyStore).toBe(false)
    expect('updateAgentLocally' in legacyStore).toBe(false)

    expect(typeof legacyStore.fetchTasks).toBe('function')
    expect(typeof legacyStore.pauseTask).toBe('function')
    expect(typeof legacyStore.resumeTask).toBe('function')
    expect(typeof legacyStore.stopTask).toBe('function')
    expect(typeof legacyStore.runTaskNow).toBe('function')
    expect(typeof legacyStore.deleteTask).toBe('function')
    expect(typeof legacyStore.convertSessionToTask).toBe('function')
    expect(typeof legacyStore.upsertTaskLocally).toBe('function')
  })

  it('only exposes the legacy factory symbol from the shim module', () => {
    expect(Object.keys(legacyStoreModule).sort()).toEqual(['useBackgroundAgentStore'])
  })
})
