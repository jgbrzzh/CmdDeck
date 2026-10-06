<script setup lang="ts">
/**
 * 通用按钮。
 *
 * 五种视觉变体（primary / default / ghost / text / danger）× 三种尺寸，
 * 覆盖全站所有按钮场景：工具栏、弹窗底部、列表项操作。
 */
import { computed, useAttrs } from "vue";

import Icon from "./Icon.vue";

defineOptions({ name: "BaseButton", inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    /** 视觉变体 */
    variant?: "primary" | "default" | "ghost" | "text" | "danger";
    /** 尺寸 */
    size?: "sm" | "md" | "lg";
    /** 左侧图标名（为空则不显示；也可改用 #icon 插槽自定义） */
    icon?: string;
    /** 加载中：显示旋转指示并禁用 */
    loading?: boolean;
    disabled?: boolean;
    /** 块级按钮，宽度 100% */
    block?: boolean;
    /** 选中/按下态（用于分段切换按钮） */
    active?: boolean;
    /** 无障碍名称，图标按钮必填 */
    label?: string;
  }>(),
  {
    variant: "default",
    size: "md",
    icon: "",
    loading: false,
    disabled: false,
    block: false,
    active: false,
    label: "",
  },
);

const emit = defineEmits<{ (e: "click", ev: MouseEvent): void }>();

const attrs = useAttrs();

/** 真正的禁用态：显式禁用或加载中 */
const inert = computed<boolean>(() => props.disabled || props.loading);

/** 只在有图标的纯图标按钮上生效（此时按钮是正方形） */
const iconOnly = computed<boolean>(() => !attrs.default);

const rootClass = computed(() => [
  "cd-btn",
  `cd-btn--${props.variant}`,
  `cd-btn--${props.size}`,
  {
    "cd-btn--block": props.block,
    "cd-btn--active": props.active,
    "cd-btn--icon-only": iconOnly.value,
    "is-loading": props.loading,
    "is-disabled": inert.value,
  },
]);

function onClick(ev: MouseEvent): void {
  if (inert.value) {
    ev.preventDefault();
    ev.stopPropagation();
    return;
  }
  emit("click", ev);
}
</script>

<template>
  <button
    v-bind="attrs"
    :class="rootClass"
    :type="(attrs.type as 'button' | 'submit' | 'reset') ?? 'button'"
    :disabled="inert"
    :title="label || undefined"
    :aria-label="label || undefined"
    :aria-busy="loading ? 'true' : undefined"
    @click="onClick"
  >
    <span v-if="loading" class="cd-btn__spinner" aria-hidden="true" />
    <Icon
      v-else-if="icon"
      class="cd-btn__icon"
      :name="icon"
      :size="size === 'sm' ? 14 : 16"
    />

    <slot name="icon" />

    <span v-if="$slots.default" class="cd-btn__text">
      <slot />
    </span>
  </button>
</template>

<style scoped>
.cd-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid var(--border, #2a2f36);
  border-radius: var(--radius-md, 4px);
  background: var(--bg-elevated, #22262d);
  color: var(--text-primary, #d7dce3);
  font-family: var(--font-ui, "Microsoft YaHei UI", sans-serif);
  font-size: 13px;
  line-height: 1;
  white-space: nowrap;
  user-select: none;
  cursor: pointer;
  transition:
    background 150ms ease-out,
    border-color 150ms ease-out,
    color 150ms ease-out;
}

.cd-btn:focus-visible {
  outline: 2px solid var(--accent, #2fd4a8);
  outline-offset: 1px;
}

/* ---------- 尺寸 ---------- */
.cd-btn--sm {
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
}
.cd-btn--md {
  height: 30px;
  padding: 0 12px;
}
.cd-btn--lg {
  height: 36px;
  padding: 0 16px;
  font-size: 14px;
}
.cd-btn--icon-only {
  width: 30px;
  padding: 0;
}
.cd-btn--sm.cd-btn--icon-only {
  width: 26px;
}
.cd-btn--lg.cd-btn--icon-only {
  width: 36px;
}
.cd-btn--block {
  width: 100%;
}

/* ---------- 变体 ---------- */
.cd-btn--primary {
  background: var(--accent, #2fd4a8);
  border-color: var(--accent, #2fd4a8);
  color: var(--text-inverse, #0d1117);
  font-weight: 600;
}
.cd-btn--primary:hover:not(.is-disabled) {
  background: var(--accent-hover, #46e0b8);
  border-color: var(--accent-hover, #46e0b8);
}

.cd-btn--default:hover:not(.is-disabled) {
  background: var(--bg-hover, #2a3038);
  border-color: var(--border-strong, #3a424c);
}

.cd-btn--ghost {
  background: transparent;
  border-color: transparent;
  color: var(--text-secondary, #9aa4b2);
}
.cd-btn--ghost:hover:not(.is-disabled) {
  background: var(--bg-hover, #2a3038);
  border-color: var(--border, #2a2f36);
  color: var(--text-primary, #d7dce3);
}

.cd-btn--text {
  background: transparent;
  border-color: transparent;
  color: var(--text-secondary, #9aa4b2);
  padding-left: 6px;
  padding-right: 6px;
}
.cd-btn--text:hover:not(.is-disabled) {
  background: var(--bg-hover, #2a3038);
  color: var(--text-primary, #d7dce3);
}

.cd-btn--danger {
  background: var(--danger, #f2545b);
  border-color: var(--danger, #f2545b);
  color: #ffffff;
  font-weight: 600;
}
.cd-btn--danger:hover:not(.is-disabled) {
  filter: brightness(1.1);
}

/* ---------- 状态 ---------- */
.cd-btn--active {
  background: var(--bg-active, #30363f);
  border-color: var(--accent, #2fd4a8);
  color: var(--accent, #2fd4a8);
}

.cd-btn.is-disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.cd-btn__text {
  display: inline-block;
}

.cd-btn__spinner {
  width: 13px;
  height: 13px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: cd-btn-spin 700ms linear infinite;
  flex: none;
}

@keyframes cd-btn-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .cd-btn,
  .cd-btn__spinner {
    transition: none;
    animation: none;
  }
}
</style>
