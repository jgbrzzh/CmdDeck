<script setup lang="ts">
/**
 * 当前会话的工具栏。
 *
 * 左：标题（可点一下重命名）/ 工作目录 / 类型徽标 / 提权徽标 / 运行时长
 * 右：清屏、复制全部、搜索、停止、关闭
 *
 * 所有"要碰到 xterm 实例"的动作都通过 `./bridge.ts` 转发给 TerminalView，
 * 本组件自己完全不持有 xterm。
 */

import { computed, onBeforeUnmount, ref, watch } from "vue";

import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type { TerminalTab } from "@/types";
import { formatClock, ellipsisPath } from "@/utils/format";
import Icon from "@/components/common/Icon.vue";
import BaseInput from "@/components/common/BaseInput.vue";
import { getBridge } from "./bridge";

const terminalStore = useTerminalStore();
const uiStore = useUiStore();

// ============================================================
// 状态
// ============================================================

/** 是否处于标题编辑态 */
const editing = ref(false);
/** 标题草稿 */
const draftTitle = ref("");
/** 运行时长（每秒刷新） */
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;

const tab = computed<TerminalTab | undefined>(
  () => terminalStore.activeTab.value,
);
const running = computed<boolean>(() => tab.value?.status === "running");

/** 已运行时长文案 */
const elapsed = computed<string>(() => {
  const t = tab.value;
  if (!t) return "00:00:00";
  return formatClock(Math.max(0, now.value - t.startedAt));
});

/** 类型徽标文本 */
const kindLabel = computed<string>(() => {
  const k = String(tab.value?.kind ?? "").toLowerCase();
  switch (k) {
    case "cmd":
      return "CMD";
    case "powershell":
      return "PowerShell";
    case "pwsh":
      return "PowerShell 7";
    case "python":
      return "Python";
    case "node":
      return "Node.js";
    case "exe":
      return "外部程序";
    case "shell":
      return "Shell";
    default:
      return k ? k.toUpperCase() : "终端";
  }
});

/** 是否提权运行 */
const elevated = computed<boolean>(() => !!tab.value?.elevated);

/** 工作目录（长路径中间省略） */
const cwdText = computed<string>(() => ellipsisPath(tab.value?.cwd ?? "", 56));

// ============================================================
// 计时器
// ============================================================

function startTimer(): void {
  stopTimer();
  now.value = Date.now();
  timer = setInterval(() => {
    now.value = Date.now();
  }, 1000);
}

function stopTimer(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

watch(
  running,
  (isRunning) => {
    if (isRunning) startTimer();
    else stopTimer();
  },
  { immediate: true },
);

onBeforeUnmount(stopTimer);

// ============================================================
// 动作
// ============================================================

function requireTab(): TerminalTab | undefined {
  const t = tab.value;
  if (!t) {
    uiStore.toast("warning", "当前没有激活的终端", "请先打开或选择一个标签页");
    return undefined;
  }
  return t;
}

/** 开始重命名 */
function startEdit(): void {
  const t = tab.value;
  if (!t) return;
  draftTitle.value = t.title || t.presetName || "终端";
  editing.value = true;
}

/** 提交重命名 */
function commitEdit(): void {
  const t = tab.value;
  const next = draftTitle.value.trim();
  if (t && next) t.title = next;
  editing.value = false;
}

/** 清屏 */
function clearScreen(): void {
  const t = requireTab();
  if (!t) return;
  getBridge(t.sessionId)?.clear();
}

/** 复制全部输出 */
async function copyAll(): Promise<void> {
  const t = requireTab();
  if (!t) return;
  const bridge = getBridge(t.sessionId);
  if (bridge) {
    bridge.copyAll();
    return;
  }
  // 视图还没挂载（例如刚打开就被点到），退化为直接读 store 缓冲
  const { writeText } = await import("@/utils/clipboard");
  const ok = await writeText(terminalStore.getBuffer(t.sessionId));
  uiStore.toast(ok ? "success" : "error", ok ? "已复制全部输出" : "复制失败");
}

/** 打开搜索条 */
function openSearch(): void {
  const t = requireTab();
  if (!t) return;
  getBridge(t.sessionId)?.search();
}

/** 终止当前进程 */
async function stopProcess(): Promise<void> {
  const t = requireTab();
  if (!t) return;
  if (!running.value) return;
  const ok = await uiStore.confirm({
    title: "终止进程",
    message: `确定要终止「${t.title || t.presetName || "终端"}」吗？`,
    detail: "终止后标签页会保留输出，可以继续查看。",
    danger: true,
    confirmText: "终止",
  });
  if (ok) await terminalStore.stop(t.sessionId);
}

/** 关闭当前标签 */
async function closeTab(): Promise<void> {
  const t = requireTab();
  if (!t) return;
  if (t.status === "running") {
    const ok = await uiStore.confirm({
      title: "关闭终端",
      message: `「${t.title || t.presetName || "终端"}」仍在运行，确定关闭吗？`,
      detail: "关闭会强制终止对应进程。",
      danger: true,
      confirmText: "关闭",
    });
    if (!ok) return;
  }
  await terminalStore.close(t.sessionId);
}
</script>

<template>
  <div v-if="tab" class="cd-tbar">
    <!-- 左：标题 / 工作目录 / 徽标 / 计时 -->
    <div class="cd-tbar__left">
      <BaseInput
        v-if="editing"
        v-model="draftTitle"
        size="sm"
        class="cd-tbar__rename"
        title="输入新的标签标题，回车确认"
        @enter="commitEdit"
        @blur="commitEdit"
        @keydown.esc.stop.prevent="editing = false"
      />
      <button
        v-else
        class="cd-tbar__title"
        type="button"
        title="点击重命名此标签"
        @click="startEdit"
      >
        <Icon name="terminal" :size="13" />
        <span class="cd-tbar__title-text">{{
          tab.title || tab.presetName || "终端"
        }}</span>
      </button>

      <span v-if="cwdText" class="cd-tbar__cwd" :title="tab.cwd">
        <Icon name="folder" :size="12" />
        <span>{{ cwdText }}</span>
      </span>

      <span class="cd-tbar__badge" :title="`会话类型：${kindLabel}`">{{
        kindLabel
      }}</span>

      <span
        v-if="elevated"
        class="cd-tbar__badge is-danger"
        title="此会话以管理员权限运行"
      >
        <Icon name="shield" :size="11" /> 管理员
      </span>

      <span
        class="cd-tbar__clock"
        :title="running ? '已运行时长' : '会话总时长'"
      >
        <span class="cd-tbar__clock-dot" :class="{ 'is-live': running }" />
        {{ elapsed }}
      </span>
    </div>

    <!-- 右：动作按钮 -->
    <div class="cd-tbar__right">
      <button
        class="cd-tbar__btn"
        type="button"
        title="清空当前屏幕（Ctrl+L 同样有效）"
        @click="clearScreen"
      >
        <Icon name="trash" :size="14" />
      </button>
      <button
        class="cd-tbar__btn"
        type="button"
        title="复制全部输出到剪贴板"
        @click="copyAll"
      >
        <Icon name="copy" :size="14" />
      </button>
      <button
        class="cd-tbar__btn"
        type="button"
        title="在输出中查找（Ctrl+Shift+F）"
        @click="openSearch"
      >
        <Icon name="search" :size="14" />
      </button>
      <button
        class="cd-tbar__btn"
        type="button"
        title="终止当前进程"
        :disabled="!running"
        @click="stopProcess"
      >
        <Icon name="stop" :size="14" />
      </button>
      <span class="cd-tbar__sep" />
      <button
        class="cd-tbar__btn is-danger"
        type="button"
        title="关闭当前标签"
        @click="closeTab"
      >
        <Icon name="x" :size="14" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.cd-tbar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
  height: 32px;
  padding: 0 6px 0 10px;
  background: var(--bg-panel, #161b22);
  border-bottom: 1px solid var(--border, #262c36);
  font-size: var(--font-size-xs, 11px);
}

.cd-tbar__left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
}

/* 标题 */
.cd-tbar__title {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 240px;
  height: 22px;
  padding: 0 7px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-primary, #e6edf3);
  font-size: var(--font-size-sm, 12px);
  font-weight: var(--font-weight-medium, 500);
  cursor: text;
  transition:
    background var(--transition-fast, 120ms),
    border-color var(--transition-fast, 120ms);
}

.cd-tbar__title:hover {
  background: var(--bg-hover, #21262d);
  border-color: var(--border, #262c36);
}

.cd-tbar__title-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cd-tbar__rename {
  width: 200px;
}

/* 工作目录 */
.cd-tbar__cwd {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  max-width: 300px;
  color: var(--text-muted, #6e7b8a);
  font-family: var(--font-mono, Consolas, monospace);
}

.cd-tbar__cwd span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 徽标 */
.cd-tbar__badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  flex: none;
  height: 18px;
  padding: 0 7px;
  border: 1px solid var(--border-accent, #2f6f6b);
  border-radius: var(--radius-pill, 999px);
  color: var(--text-accent, #5eead4);
  background: var(--accent-soft, rgba(45, 212, 191, 0.14));
  font-size: 10px;
  white-space: nowrap;
}

.cd-tbar__badge.is-danger {
  border-color: var(--danger, #f85149);
  background: var(--danger-soft, rgba(248, 81, 73, 0.14));
  color: var(--danger, #f85149);
}

/* 计时器 */
.cd-tbar__clock {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex: none;
  color: var(--text-muted, #6e7b8a);
  font-family: var(--font-mono, Consolas, monospace);
  user-select: none;
}

.cd-tbar__clock-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--text-muted, #6e7b8a);
}

.cd-tbar__clock-dot.is-live {
  background: var(--success, #3fb950);
  animation: cd-tbar-pulse 1.6s ease-in-out infinite;
}

@keyframes cd-tbar-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.3;
  }
}

@media (prefers-reduced-motion: reduce) {
  .cd-tbar__clock-dot.is-live {
    animation: none;
  }
}

/* 右侧按钮 */
.cd-tbar__right {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
}

.cd-tbar__sep {
  width: 1px;
  height: 14px;
  margin: 0 3px;
  background: var(--border, #262c36);
}

.cd-tbar__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 22px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-secondary, #a6b3c2);
  cursor: pointer;
  padding: 0;
  transition:
    background var(--transition-fast, 120ms),
    color var(--transition-fast, 120ms),
    border-color var(--transition-fast, 120ms);
}

.cd-tbar__btn:hover:not(:disabled) {
  background: var(--bg-hover, #21262d);
  border-color: var(--border, #262c36);
  color: var(--text-primary, #e6edf3);
}

.cd-tbar__btn.is-danger:hover:not(:disabled) {
  background: var(--danger-soft, rgba(248, 81, 73, 0.14));
  border-color: var(--danger, #f85149);
  color: var(--danger, #f85149);
}

.cd-tbar__btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
