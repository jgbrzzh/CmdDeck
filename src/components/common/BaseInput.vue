<script setup lang="ts">
/**
 * 通用单行输入框。
 *
 * 支持前缀/后缀插槽（图标）、一键清除、错误提示、number 类型的 min/max/step。
 * 组件内部维护 `draft`：用户还没按回车/失焦时也能实时回写 model（受控用法）。
 */
import { computed, ref, useAttrs, watch } from "vue";

import Icon from "./Icon.vue";

defineOptions({ name: "BaseInput", inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    modelValue: string | number;
    placeholder?: string;
    /** 原生 input type */
    type?: string;
    disabled?: boolean;
    readonly?: boolean;
    /** 右侧显示清除按钮 */
    clearable?: boolean;
    /** 错误文案，非空时输入框转红 */
    error?: string;
    min?: number;
    max?: number;
    step?: number;
    size?: "sm" | "md";
  }>(),
  {
    placeholder: "",
    type: "text",
    disabled: false,
    readonly: false,
    clearable: false,
    error: "",
    min: undefined,
    max: undefined,
    step: undefined,
    size: "md",
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string | number): void;
  (e: "enter"): void;
  (e: "blur", ev: FocusEvent): void;
  (e: "focus", ev: FocusEvent): void;
}>();

const attrs = useAttrs();
const inputRef = ref<HTMLInputElement | null>(null);

/** 内部草稿值，透传 modelValue 的初始值 */
const draft = ref<string>(
  props.modelValue === null || props.modelValue === undefined
    ? ""
    : String(props.modelValue),
);

// 外部把 model 换掉时（例如切换了预设），同步草稿
watch(
  () => props.modelValue,
  (v) => {
    const next = v === null || v === undefined ? "" : String(v);
    if (next !== draft.value) draft.value = next;
  },
);

/** 是否可以点清除按钮 */
const canClear = computed<boolean>(
  () =>
    props.clearable && !props.disabled && !props.readonly && draft.value !== "",
);

function commit(): void {
  const next = draft.value;
  if (props.type === "number") {
    if (next === "") {
      emit("update:modelValue", "");
      return;
    }
    const n = Number(next);
    if (Number.isNaN(n)) {
      emit("update:modelValue", next);
      return;
    }
    // min/max 约束仅在原生属性上生效，这里再夹一次避免外部拿到越界值
    let v = n;
    if (typeof props.min === "number" && v < props.min) v = props.min;
    if (typeof props.max === "number" && v > props.max) v = props.max;
    emit("update:modelValue", v);
    return;
  }
  emit("update:modelValue", next);
}

function onInput(ev: Event): void {
  draft.value = (ev.target as HTMLInputElement).value;
  commit();
}

function onKeydown(ev: KeyboardEvent): void {
  // 冒泡给外部插槽/表单使用
  if (typeof attrs.onkeydown === "function") attrs.onkeydown(ev);
  if (ev.key === "Enter") {
    commit();
    emit("enter");
  }
}

function clear(): void {
  draft.value = "";
  emit("update:modelValue", props.type === "number" ? "" : "");
  inputRef.value?.focus();
}

function focus(): void {
  inputRef.value?.focus();
}

defineExpose({ focus, inputRef });
</script>

<template>
  <div
    class="cd-input"
    :class="[
      `cd-input--${size}`,
      {
        'is-disabled': disabled,
        'is-error': !!error,
        'has-suffix': canClear || !!$slots.suffix,
      },
    ]"
  >
    <span v-if="$slots.prefix" class="cd-input__affix cd-input__affix--prefix">
      <slot name="prefix" />
    </span>

    <input
      ref="inputRef"
      v-bind="attrs"
      class="cd-input__control"
      :type="type"
      :value="draft"
      :placeholder="placeholder"
      :disabled="disabled"
      :readonly="readonly"
      :min="min"
      :max="max"
      :step="step"
      autocomplete="off"
      spellcheck="false"
      @input="onInput"
      @keydown="onKeydown"
      @blur="emit('blur', $event)"
      @focus="emit('focus', $event)"
    />

    <span
      v-if="canClear"
      class="cd-input__affix cd-input__affix--suffix"
      @mousedown.prevent
    >
      <button
        class="cd-input__clear"
        type="button"
        title="清除"
        aria-label="清除"
        @click="clear"
      >
        <Icon name="x" :size="12" />
      </button>
    </span>

    <span
      v-else-if="$slots.suffix"
      class="cd-input__affix cd-input__affix--suffix"
    >
      <slot name="suffix" />
    </span>

    <p v-if="error" class="cd-input__error">{{ error }}</p>
  </div>
</template>

<style scoped>
.cd-input {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  background: var(--bg-input, #1b1f25);
  border: 1px solid var(--border, #2a2f36);
  border-radius: var(--radius-md, 4px);
  padding: 0 8px;
  transition:
    border-color 150ms ease-out,
    background 150ms ease-out;
}

.cd-input--sm {
  height: 26px;
  font-size: 12px;
}
.cd-input--md {
  height: 30px;
  font-size: 13px;
}

.cd-input:focus-within {
  border-color: var(--accent, #2fd4a8);
  background: var(--bg-elevated, #22262d);
}

.cd-input.is-error {
  border-color: var(--danger, #f2545b);
}

.cd-input.is-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.cd-input__control {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary, #d7dce3);
  font-family: inherit;
  font-size: inherit;
  padding: 0;
}

.cd-input__control::placeholder {
  color: var(--text-muted, #6b7482);
}

/* 隐藏 number 类型输入框的上下箭头，样式由我们自己的 min/max 约束负责 */
.cd-input__control[type="number"]::-webkit-outer-spin-button,
.cd-input__control[type="number"]::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}
.cd-input__control[type="number"] {
  -moz-appearance: textfield;
  appearance: textfield;
}

.cd-input__affix {
  display: inline-flex;
  align-items: center;
  color: var(--text-muted, #6b7482);
  flex: none;
}

.cd-input__clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-muted, #6b7482);
  cursor: pointer;
  padding: 0;
  transition:
    background 150ms ease-out,
    color 150ms ease-out;
}

.cd-input__clear:hover {
  background: var(--bg-hover, #2a3038);
  color: var(--text-primary, #d7dce3);
}

.cd-input__error {
  flex: 1 0 100%;
  margin: 0;
  padding: 2px 0 4px;
  font-size: 11px;
  line-height: 1.3;
  color: var(--danger, #f2545b);
}
</style>
