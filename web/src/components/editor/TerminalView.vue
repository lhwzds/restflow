<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebglAddon } from '@xterm/addon-webgl'
import {
  spawnPty,
  writePty,
  resizePty,
  onPtyOutput,
  onPtyClosed,
  getPtyStatus,
  getPtyHistory,
} from '@/api/pty'
import { isTauri } from '@/api/tauri-client'
import type { TerminalSession } from '@/types/generated/TerminalSession'
import '@xterm/xterm/css/xterm.css'

const isDemoMode = import.meta.env.VITE_DEMO_MODE === 'true'

const props = defineProps<{
  tabId: string
  session: TerminalSession
}>()

const terminalRef = ref<HTMLElement | null>(null)
const isConnected = ref(false)
const isReadonly = ref(false)
const error = ref<string | null>(null)

let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let unlistenOutput: (() => void) | null = null
let unlistenClosed: (() => void) | null = null
let resizeObserver: ResizeObserver | null = null

// Terminal theme configuration
const terminalTheme = {
  background: '#09090b',
  foreground: '#fafafa',
  cursor: '#fafafa',
  cursorAccent: '#09090b',
  selectionBackground: '#3f3f46',
  black: '#09090b',
  red: '#ef4444',
  green: '#22c55e',
  yellow: '#eab308',
  blue: '#3b82f6',
  magenta: '#a855f7',
  cyan: '#06b6d4',
  white: '#fafafa',
  brightBlack: '#71717a',
  brightRed: '#f87171',
  brightGreen: '#4ade80',
  brightYellow: '#facc15',
  brightBlue: '#60a5fa',
  brightMagenta: '#c084fc',
  brightCyan: '#22d3ee',
  brightWhite: '#ffffff',
}

/**
 * Initialize demo terminal with simulated output
 */
function initDemoTerminal() {
  if (!terminalRef.value) return

  term = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: '"SF Mono", Menlo, Monaco, "Cascadia Code", "Segoe UI Mono", "Roboto Mono", "Oxygen Mono", "Ubuntu Monospace", "Source Code Pro", "Fira Mono", "Droid Sans Mono", "Courier New", monospace',
    theme: terminalTheme,
    disableStdin: true,
    letterSpacing: 0,
    lineHeight: 1.0,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  const unicode11Addon = new Unicode11Addon()
  term.loadAddon(fitAddon)
  term.loadAddon(unicode11Addon)
  term.unicode.activeVersion = '11'
  term.open(terminalRef.value)

  try {
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
  } catch (e) {
    console.warn('WebGL addon failed to load, using canvas renderer:', e)
  }

  fitAddon.fit()

  // Write simulated terminal output
  const demoOutput = [
    '\x1b[32m➜\x1b[0m \x1b[36m~/restflow\x1b[0m \x1b[33mgit:(\x1b[0m\x1b[31mmain\x1b[0m\x1b[33m)\x1b[0m ',
    'cargo run --bin restflow-server\r\n',
    '\x1b[32m   Compiling\x1b[0m restflow-core v0.1.0\r\n',
    '\x1b[32m   Compiling\x1b[0m restflow-server v0.1.0\r\n',
    '\x1b[32m    Finished\x1b[0m dev [unoptimized + debuginfo] target(s) in 2.34s\r\n',
    '\x1b[32m     Running\x1b[0m `target/debug/restflow-server`\r\n',
    '\r\n',
    '\x1b[34m[INFO]\x1b[0m RestFlow server starting...\r\n',
    '\x1b[34m[INFO]\x1b[0m Database initialized at ./restflow.db\r\n',
    '\x1b[34m[INFO]\x1b[0m API server listening on http://127.0.0.1:3000\r\n',
    '\x1b[32m[READY]\x1b[0m RestFlow is ready to accept connections\r\n',
    '\r\n',
    '\x1b[33m[DEMO]\x1b[0m This is a simulated terminal preview.\r\n',
    '\x1b[33m[DEMO]\x1b[0m Full terminal access requires the desktop app.\r\n',
  ]

  demoOutput.forEach((line) => term?.write(line))

  isConnected.value = true

  resizeObserver = new ResizeObserver(() => {
    if (fitAddon) {
      fitAddon.fit()
    }
  })
  resizeObserver.observe(terminalRef.value)
}

/**
 * Initialize readonly terminal for viewing history
 */
function initReadonlyTerminal() {
  if (!terminalRef.value) return

  term = new Terminal({
    cursorBlink: false,
    fontSize: 14,
    fontFamily: '"SF Mono", Menlo, Monaco, "Cascadia Code", "Segoe UI Mono", "Roboto Mono", "Oxygen Mono", "Ubuntu Monospace", "Source Code Pro", "Fira Mono", "Droid Sans Mono", "Courier New", monospace',
    theme: terminalTheme,
    disableStdin: true, // Disable input for readonly mode
    letterSpacing: 0,
    lineHeight: 1.0,
    allowProposedApi: true, // Required for unicode11 addon
  })

  fitAddon = new FitAddon()
  const unicode11Addon = new Unicode11Addon()
  term.loadAddon(fitAddon)
  term.loadAddon(unicode11Addon)
  term.unicode.activeVersion = '11'
  term.open(terminalRef.value)

  // Try to load WebGL addon for better rendering
  try {
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
  } catch (e) {
    console.warn('WebGL addon failed to load, using canvas renderer:', e)
  }

  fitAddon.fit()

  // Write history content
  if (props.session.history) {
    term.write(props.session.history)
  }

  // Add stopped message
  term.write('\r\n\x1b[33m[Terminal stopped - history only]\x1b[0m\r\n')

  isReadonly.value = true

  // Handle resize
  resizeObserver = new ResizeObserver(() => {
    if (fitAddon) {
      fitAddon.fit()
    }
  })
  resizeObserver.observe(terminalRef.value)
}

/**
 * Initialize interactive terminal with PTY
 */
async function initInteractiveTerminal() {
  if (!terminalRef.value) return

  term = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: '"SF Mono", Menlo, Monaco, "Cascadia Code", "Segoe UI Mono", "Roboto Mono", "Oxygen Mono", "Ubuntu Monospace", "Source Code Pro", "Fira Mono", "Droid Sans Mono", "Courier New", monospace',
    theme: terminalTheme,
    letterSpacing: 0,
    lineHeight: 1.0,
    allowProposedApi: true, // Required for unicode11 addon
  })

  fitAddon = new FitAddon()
  const unicode11Addon = new Unicode11Addon()
  term.loadAddon(fitAddon)
  term.loadAddon(unicode11Addon)
  term.unicode.activeVersion = '11'
  term.open(terminalRef.value)

  // Try to load WebGL addon for better rendering
  try {
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
  } catch (e) {
    console.warn('WebGL addon failed to load, using canvas renderer:', e)
  }

  fitAddon.fit()

  // Listen for PTY output
  unlistenOutput = await onPtyOutput(props.tabId, (data) => {
    term?.write(data)
  })

  // Listen for PTY closed
  unlistenClosed = await onPtyClosed(props.tabId, () => {
    term?.write('\r\n\x1b[31m[Process exited]\x1b[0m\r\n')
    isConnected.value = false
  })

  // Check if PTY is already running (e.g., tab switch)
  const isPtyRunning = await getPtyStatus(props.tabId)

  if (isPtyRunning) {
    // PTY is running, restore history first then attach
    // This fixes Bug #2: closing and reopening a tab should show previous output
    try {
      const history = await getPtyHistory(props.tabId)
      if (history) {
        term?.write(history)
      }
    } catch (e) {
      console.warn('Failed to restore terminal history:', e)
    }
    isConnected.value = true
  } else {
    // Spawn new PTY session
    await spawnPty(props.tabId, term.cols, term.rows)
    isConnected.value = true
  }

  // Send user input to PTY
  term.onData((data) => {
    if (isConnected.value) {
      writePty(props.tabId, data)
    }
  })

  // Handle window resize
  resizeObserver = new ResizeObserver(() => {
    if (fitAddon && term && isConnected.value) {
      fitAddon.fit()
      resizePty(props.tabId, term.cols, term.rows)
    }
  })
  resizeObserver.observe(terminalRef.value)

  // Focus terminal
  term.focus()
}

onMounted(async () => {
  if (!terminalRef.value) return

  // Check if running in Tauri (skip check in demo mode - we'll show a preview)
  if (!isTauri() && !isDemoMode) {
    error.value = 'Terminal requires Tauri desktop app'
    return
  }

  // In demo mode, show a simulated terminal
  if (isDemoMode) {
    initDemoTerminal()
    return
  }

  try {
    if (props.session.status === 'stopped') {
      // Readonly mode: show history only
      initReadonlyTerminal()
    } else {
      // Interactive mode: connect to PTY
      await initInteractiveTerminal()
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    console.error('Failed to initialize terminal:', e)
  }
})

onUnmounted(() => {
  // Cleanup - but DON'T close the PTY!
  // PTY should continue running in the background
  resizeObserver?.disconnect()
  unlistenOutput?.()
  unlistenClosed?.()
  term?.dispose()
})

// Watch for session status changes (e.g., after restart)
watch(
  () => props.session.status,
  async (newStatus, oldStatus) => {
    if (oldStatus === 'stopped' && newStatus === 'running') {
      // Session was restarted, reinitialize as interactive
      term?.dispose()
      unlistenOutput?.()
      unlistenClosed?.()
      resizeObserver?.disconnect()

      isReadonly.value = false
      error.value = null

      try {
        await initInteractiveTerminal()
      } catch (e) {
        error.value = e instanceof Error ? e.message : String(e)
        console.error('Failed to restart terminal:', e)
      }
    }
  },
)
</script>

<template>
  <div class="h-full w-full bg-zinc-950 relative">
    <!-- Terminal container -->
    <div ref="terminalRef" class="h-full w-full" />

    <!-- Readonly indicator -->
    <div
      v-if="isReadonly"
      class="absolute top-2 right-2 px-2 py-1 bg-yellow-500/20 text-yellow-400 text-xs rounded"
    >
      History Only
    </div>

    <!-- Demo mode indicator -->
    <div
      v-if="isDemoMode && !error"
      class="absolute top-2 right-2 px-2 py-1 bg-blue-500/20 text-blue-400 text-xs rounded"
    >
      Demo Preview
    </div>

    <!-- Error overlay -->
    <div
      v-if="error"
      class="absolute inset-0 flex items-center justify-center bg-zinc-950/90 text-red-400"
    >
      <div class="text-center p-4">
        <p class="text-lg font-medium mb-2">Terminal Error</p>
        <p class="text-sm text-zinc-400">{{ error }}</p>
      </div>
    </div>
  </div>
</template>

<style>
/* Ensure xterm fills the container */
.xterm {
  height: 100%;
  padding: 8px;
}

.xterm-viewport {
  overflow-y: auto !important;
}
</style>
