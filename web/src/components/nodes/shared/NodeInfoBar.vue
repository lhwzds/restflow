<script setup lang="ts">
import { computed, inject } from 'vue'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'
import type { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'

interface Props {
  nodeId: string
}

const props = defineProps<Props>()

const executionStatus = useNodeExecutionStatus()
const executionTime = computed(() => {
  const time = executionStatus.getNodeExecutionTime(props.nodeId)
  return time ? executionStatus.formatExecutionTime(time) : null
})

// Inject popup state from BaseNode
const popupState = inject<ReturnType<typeof useNodeInfoPopup>>('nodePopupState')!
const { hasInput, hasOutput, showInputPopup, showTimePopup, showOutputPopup, activeTab } =
  popupState
</script>

<template>
  <div v-if="executionTime || hasInput() || hasOutput()" class="node-info-tags">
    <span
      v-if="hasInput()"
      class="info-tag input"
      :class="{ active: activeTab !== null && activeTab === 'input' }"
      @click="showInputPopup"
    >
      Input
    </span>
    <span
      v-if="executionTime"
      class="info-tag time"
      :class="{ active: activeTab !== null && activeTab === 'time' }"
      @click="showTimePopup"
    >
      {{ executionTime }}
    </span>
    <span
      v-if="hasOutput()"
      class="info-tag output"
      :class="{ active: activeTab !== null && activeTab === 'output' }"
      @click="showOutputPopup"
    >
      Output
    </span>
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/node-info-tags' as *;

@include node-info-tags();
</style>
