<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { Play, Pause, Mic } from 'lucide-vue-next'

const props = defineProps<{
  blobUrl: string
  duration: number
}>()

const audio = ref<HTMLAudioElement | null>(null)
const isPlaying = ref(false)
const currentTime = ref(0)
const audioDuration = ref(props.duration)

const displayDuration = computed(() => formatTime(audioDuration.value))
const displayCurrent = computed(() => formatTime(Math.floor(currentTime.value)))
const progress = computed(() =>
  audioDuration.value > 0 ? (currentTime.value / audioDuration.value) * 100 : 0,
)

function formatTime(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

function toggle() {
  if (!audio.value) return
  if (isPlaying.value) {
    audio.value.pause()
  } else {
    audio.value.play()
  }
}

function onTimeUpdate() {
  if (audio.value) {
    currentTime.value = audio.value.currentTime
  }
}

function onEnded() {
  isPlaying.value = false
  currentTime.value = 0
}

onMounted(() => {
  audio.value = new Audio(props.blobUrl)
  audio.value.addEventListener('timeupdate', onTimeUpdate)
  audio.value.addEventListener('play', () => (isPlaying.value = true))
  audio.value.addEventListener('pause', () => (isPlaying.value = false))
  audio.value.addEventListener('ended', onEnded)
  audio.value.addEventListener('loadedmetadata', () => {
    if (audio.value && isFinite(audio.value.duration) && audio.value.duration > 0) {
      audioDuration.value = Math.floor(audio.value.duration)
    }
  })
})

onUnmounted(() => {
  if (audio.value) {
    audio.value.pause()
    audio.value.removeEventListener('timeupdate', onTimeUpdate)
    audio.value.removeEventListener('ended', onEnded)
    audio.value = null
  }
})
</script>

<template>
  <div class="flex items-center gap-2.5 min-w-[180px]">
    <!-- Play/Pause button -->
    <button
      class="flex items-center justify-center w-8 h-8 rounded-full bg-primary text-primary-foreground shrink-0 hover:opacity-90 transition-opacity"
      @click="toggle"
    >
      <Pause v-if="isPlaying" :size="14" />
      <Play v-else :size="14" class="ml-0.5" />
    </button>

    <!-- Waveform / progress area -->
    <div class="flex-1 flex flex-col gap-1">
      <!-- Progress bar -->
      <div class="h-1 rounded-full bg-muted-foreground/20 overflow-hidden">
        <div
          class="h-full rounded-full bg-primary transition-[width] duration-100"
          :style="{ width: `${progress}%` }"
        />
      </div>
      <!-- Time -->
      <div class="flex items-center justify-between text-[10px] text-muted-foreground">
        <span class="tabular-nums">{{ isPlaying ? displayCurrent : displayDuration }}</span>
        <Mic :size="10" class="opacity-50" />
      </div>
    </div>
  </div>
</template>
