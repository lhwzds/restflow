<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { spawnPty, writePty, resizePty, closePty, onPtyOutput, onPtyClosed } from '@/api/pty'
import { isTauri } from '@/api/tauri-client'
import '@xterm/xterm/css/xterm.css'

const props = defineProps<{
  tabId: string
}>()

const terminalRef = ref<HTMLElement | null>(null)
const isConnected = ref(false)
const error = ref<string | null>(null)

let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let unlistenOutput: (() => void) | null = null
let unlistenClosed: (() => void) | null = null
let resizeObserver: ResizeObserver | null = null

onMounted(async () => {
  if (!terminalRef.value) return

  // Check if running in Tauri
  if (!isTauri()) {
    error.value = 'Terminal requires Tauri desktop app'
    return
  }

  try {
    // Initialize xterm.js
    term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      theme: {
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
      },
    })

    // Add fit addon for auto-resizing
    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)

    // Open terminal in DOM
    term.open(terminalRef.value)
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

    // Spawn PTY session
    await spawnPty(props.tabId, term.cols, term.rows)
    isConnected.value = true

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
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    console.error('Failed to initialize terminal:', e)
  }
})

onUnmounted(async () => {
  // Cleanup
  resizeObserver?.disconnect()
  unlistenOutput?.()
  unlistenClosed?.()

  if (isConnected.value) {
    try {
      await closePty(props.tabId)
    } catch (e) {
      console.error('Failed to close PTY:', e)
    }
  }

  term?.dispose()
})
</script>

<template>
  <div class="h-full w-full bg-zinc-950 relative">
    <!-- Terminal container -->
    <div ref="terminalRef" class="h-full w-full" />

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
