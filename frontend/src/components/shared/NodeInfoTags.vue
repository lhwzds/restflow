<script setup lang="ts">
import { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'
import NodeInfoPopup from '@/components/nodes/NodeInfoPopup.vue'

interface Props {
  nodeId: string
  executionTime: string | null
}

const props = defineProps<Props>()

const {
  popupVisible,
  popupType,
  popupPosition,
  nodeResult,
  activeTab,
  hasInput,
  hasOutput,
  showTimePopup,
  showInputPopup,
  showOutputPopup,
  closePopup
} = useNodeInfoPopup(props.nodeId)
</script>

<template>
  <!-- Node info bar - independent tags -->
  <div v-if="executionTime || hasInput() || hasOutput()" class="node-info-tags">
    <span
      v-if="hasInput()"
      class="info-tag input"
      :class="{ active: activeTab === 'input' }"
      @click="showInputPopup"
    >
      Input
    </span>
    <span
      v-if="executionTime"
      class="info-tag time"
      :class="{ active: activeTab === 'time' }"
      @click="showTimePopup"
    >
      {{ executionTime }}
    </span>
    <span
      v-if="hasOutput()"
      class="info-tag output"
      :class="{ active: activeTab === 'output' }"
      @click="showOutputPopup"
    >
      Output
    </span>
  </div>

  <!-- Info popup -->
  <NodeInfoPopup
    :visible="popupVisible"
    :type="popupType"
    :data="nodeResult()"
    :position="popupPosition"
    @close="closePopup"
  />
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/node-info-tags' as *;

// Include shared node info tags styles
@include node-info-tags();
</style>