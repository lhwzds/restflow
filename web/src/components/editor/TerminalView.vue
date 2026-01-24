<script setup lang="ts">
import { ref, nextTick, onMounted } from 'vue'
import { Loader2 } from 'lucide-vue-next'

const props = defineProps<{
  tabId: string
}>()

interface OutputLine {
  type: 'input' | 'output' | 'error'
  content: string
}

const output = ref<OutputLine[]>([])
const inputValue = ref('')
const isRunning = ref(false)
const outputContainer = ref<HTMLElement | null>(null)

// Scroll to bottom when new output is added
async function scrollToBottom() {
  await nextTick()
  if (outputContainer.value) {
    outputContainer.value.scrollTop = outputContainer.value.scrollHeight
  }
}

// Handle command submission
async function handleSubmit() {
  const command = inputValue.value.trim()
  if (!command || isRunning.value) return

  // Add command to output
  output.value.push({ type: 'input', content: `$ ${command}` })
  inputValue.value = ''
  isRunning.value = true
  await scrollToBottom()

  try {
    // TODO: Integrate with Tauri command execution
    // For now, simulate command execution
    await new Promise((resolve) => setTimeout(resolve, 500))

    // Placeholder response
    output.value.push({
      type: 'output',
      content: `[Command execution not yet implemented]\nCommand: ${command}`,
    })
  } catch (error) {
    output.value.push({
      type: 'error',
      content: error instanceof Error ? error.message : 'Unknown error',
    })
  } finally {
    isRunning.value = false
    await scrollToBottom()
  }
}

// Handle key events
function handleKeyDown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    handleSubmit()
  }
}

onMounted(() => {
  // Add welcome message
  output.value.push({
    type: 'output',
    content: 'Terminal ready. Type a command and press Enter.',
  })
})
</script>

<template>
  <div class="h-full flex flex-col bg-zinc-950 text-zinc-100 font-mono text-sm">
    <!-- Output Area -->
    <div ref="outputContainer" class="flex-1 overflow-auto p-4 space-y-1">
      <div
        v-for="(line, index) in output"
        :key="index"
        :class="{
          'text-zinc-400': line.type === 'output',
          'text-green-400': line.type === 'input',
          'text-red-400': line.type === 'error',
        }"
      >
        <pre class="whitespace-pre-wrap break-words">{{ line.content }}</pre>
      </div>

      <!-- Loading indicator -->
      <div v-if="isRunning" class="flex items-center gap-2 text-zinc-500">
        <Loader2 :size="14" class="animate-spin" />
        <span>Running...</span>
      </div>
    </div>

    <!-- Input Area -->
    <div class="border-t border-zinc-800 p-2 flex items-center gap-2">
      <span class="text-green-400">$</span>
      <input
        v-model="inputValue"
        type="text"
        class="flex-1 bg-transparent outline-none text-zinc-100 placeholder:text-zinc-600"
        placeholder="Enter command..."
        :disabled="isRunning"
        @keydown="handleKeyDown"
      />
    </div>
  </div>
</template>
