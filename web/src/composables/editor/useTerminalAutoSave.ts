/**
 * Terminal Auto-Save
 *
 * Periodically saves terminal history for running terminals.
 * This ensures that terminal output is preserved even if the app crashes.
 */

import { onMounted, onUnmounted } from 'vue'
import { useTerminalSessions } from './useTerminalSessions'
import { saveTerminalHistory, saveAllTerminalHistory, getPtyStatus } from '@/api/pty'

// Save interval in milliseconds (30 seconds)
const SAVE_INTERVAL = 30_000

// Track if auto-save is enabled
let intervalId: ReturnType<typeof setInterval> | null = null
let isInitialized = false

/**
 * Save history for all running terminals
 */
async function saveAllRunningTerminals() {
  const { runningSessions } = useTerminalSessions()

  const sessions = runningSessions.value
  console.debug(`[TerminalAutoSave] Checking ${sessions.length} running sessions`)

  for (const session of sessions) {
    try {
      // Check if PTY is still running before saving
      const isRunning = await getPtyStatus(session.id)
      console.debug(`[TerminalAutoSave] Session ${session.id}: PTY running = ${isRunning}`)

      if (isRunning) {
        await saveTerminalHistory(session.id)
        console.debug(`[TerminalAutoSave] Saved history for ${session.id}`)
      }
    } catch (error) {
      console.warn(`[TerminalAutoSave] Failed to save terminal history for ${session.id}:`, error)
    }
  }
}

/**
 * Start the auto-save interval
 */
function startAutoSave() {
  if (intervalId !== null) {
    return // Already started
  }

  intervalId = setInterval(saveAllRunningTerminals, SAVE_INTERVAL)
  console.debug('Terminal auto-save started')
}

/**
 * Stop the auto-save interval
 */
function stopAutoSave() {
  if (intervalId !== null) {
    clearInterval(intervalId)
    intervalId = null
    console.debug('Terminal auto-save stopped')
  }
}

/**
 * Handle app beforeunload event to save all terminals
 */
async function handleBeforeUnload() {
  try {
    await saveAllTerminalHistory()
  } catch (error) {
    console.error('Failed to save terminal history on unload:', error)
  }
}

/**
 * Composable to enable terminal auto-save
 *
 * Should be called once in the root component (e.g., SkillWorkspace.vue)
 */
export function useTerminalAutoSave() {
  onMounted(() => {
    if (!isInitialized) {
      startAutoSave()

      // Add beforeunload handler
      window.addEventListener('beforeunload', handleBeforeUnload)

      isInitialized = true
    }
  })

  onUnmounted(() => {
    // Note: We don't stop auto-save on unmount because this should be
    // called from the root component. The auto-save will continue until
    // the app is closed.
  })

  return {
    saveAllRunningTerminals,
    startAutoSave,
    stopAutoSave,
  }
}
