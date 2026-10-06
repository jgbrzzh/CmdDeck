<script setup lang="ts">
/**
 * 通用图标组件。
 *
 * 全站不引第三方图标库，所有图标都是本文件 `icons.ts` 里的内联 SVG：
 * - 描边统一 `currentColor`，颜色跟随父元素 `color`，天然适配明暗主题；
 * - 未知图标名渲染一个方块占位并 `console.warn`，不会白屏或抛错。
 */
import { computed, onMounted, watch } from "vue";

import { FALLBACK_ICON, ICON_PATHS } from "./icons";

const props = withDefaults(
  defineProps<{
    /** 图标名，取值见 `icons.ts` 的 ICON_PATHS */
    name: string;
    /** 像素尺寸，默认 18 */
    size?: number;
    /** 描边宽度，默认 1.8 */
    strokeWidth?: number;
  }>(),
  {
    size: 18,
    strokeWidth: 1.8,
  },
);

/** 实际要渲染的片段；找不到就退化成占位方块 */
const body = computed<string>(
  () => ICON_PATHS[props.name] ?? ICON_PATHS[FALLBACK_ICON] ?? "",
);

/** 是否命中了未知图标（只 warn 一次，避免列表里刷屏） */
const known = computed<boolean>(() => props.name in ICON_PATHS);
const warned = new Set<string>();

function warnUnknown(name: string): void {
  if (warned.has(name)) return;
  warned.add(name);
  console.warn(
    `[Icon] 未收录的图标名：${name}，已使用占位方块。可在 src/components/common/icons.ts 中补充。`,
  );
}

onMounted(() => {
  if (!known.value) warnUnknown(props.name);
});

watch(
  () => props.name,
  (n) => {
    if (!(n in ICON_PATHS)) warnUnknown(n);
  },
  { immediate: true },
);
</script>

<template>
  <svg
    class="cd-icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
    v-html="body"
  />
</template>

<style scoped>
.cd-icon {
  display: inline-block;
  flex: none;
  vertical-align: -0.15em;
  overflow: visible;
  pointer-events: none;
}
</style>
