import { defineComponent, h } from 'vue'

/**
 * Mock ElDialog component
 * Supports v-model, slots, and events without Teleport issues
 */
export const ElDialogStub = defineComponent({
  name: 'ElDialog',
  props: {
    modelValue: Boolean,
    title: String,
  },
  emits: ['update:modelValue', 'close'],
  setup(props, { slots, emit }) {
    return () => {
      if (!props.modelValue) return null

      return h('div', { 'data-test': 'el-dialog', class: 'el-dialog' }, [
        props.title ? h('div', { class: 'el-dialog__header' }, props.title) : null,
        h('div', { class: 'el-dialog__body' }, slots.default?.()),
        h('div', { class: 'el-dialog__footer' }, slots.footer?.()),
      ])
    }
  },
})

/**
 * Mock ElForm component
 */
export const ElFormStub = defineComponent({
  name: 'ElForm',
  props: {
    model: Object,
    rules: Object,
  },
  setup(props, { slots }) {
    return () => h('form', { class: 'el-form' }, slots.default?.())
  },
})

/**
 * Mock ElFormItem component
 */
export const ElFormItemStub = defineComponent({
  name: 'ElFormItem',
  props: {
    label: String,
    prop: String,
  },
  setup(props, { slots }) {
    return () =>
      h('div', { class: 'el-form-item' }, [
        props.label ? h('label', { class: 'el-form-item__label' }, props.label) : null,
        h('div', { class: 'el-form-item__content' }, slots.default?.()),
      ])
  },
})

/**
 * Mock ElInput component
 * Supports v-model
 */
export const ElInputStub = defineComponent({
  name: 'ElInput',
  props: {
    modelValue: [String, Number],
    placeholder: String,
    type: {
      type: String,
      default: 'text',
    },
  },
  emits: ['update:modelValue', 'input', 'change'],
  setup(props, { emit }) {
    const handleInput = (e: Event) => {
      const value = (e.target as HTMLInputElement).value
      emit('update:modelValue', value)
      emit('input', value)
    }

    const handleChange = (e: Event) => {
      const value = (e.target as HTMLInputElement).value
      emit('change', value)
    }

    return () =>
      h('input', {
        class: 'el-input__inner',
        value: props.modelValue,
        placeholder: props.placeholder,
        type: props.type,
        onInput: handleInput,
        onChange: handleChange,
      })
  },
})

/**
 * Mock ElButton component
 */
export const ElButtonStub = defineComponent({
  name: 'ElButton',
  props: {
    type: String,
    disabled: Boolean,
  },
  setup(props, { slots }) {
    return () =>
      h(
        'button',
        {
          class: `el-button ${props.type ? `el-button--${props.type}` : ''}`,
          disabled: props.disabled,
        },
        slots.default?.()
      )
  },
})

/**
 * Mock ElMessage service
 */
export const mockElMessage = {
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}
