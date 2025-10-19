<script setup lang="ts">
import { ElSwitch, ElTooltip } from 'element-plus'

interface Props {
  isActive: boolean
  isLoading?: boolean
  statusText?: string
  size?: 'small' | 'default' | 'large'
}

withDefaults(defineProps<Props>(), {
  isLoading: false,
  statusText: '',
  size: 'large',
})

defineEmits<{
  toggle: []
}>()
</script>

<template>
  <div class="trigger-toggle">
    <el-tooltip
      :content="isActive ? 'Deactivate workflow' : 'Activate workflow'"
      placement="bottom"
    >
      <el-switch
        :model-value="isActive"
        :loading="isLoading"
        :active-text="statusText"
        :size="size"
        @change="$emit('toggle')"
      />
    </el-tooltip>
  </div>
</template>

<style lang="scss" scoped>
.trigger-toggle {
  display: flex;
  align-items: center;
}
</style>
