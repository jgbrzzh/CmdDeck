<script setup lang="ts">
/**
 * 多标签条。
 *
 * - 横向可滚动，标签过多时出现左右滚动按钮
 * - 每个标签：图标 + 标题 + 状态圆点 + 关闭按钮（hover 出现）
 * - 右键标签 → 菜单（重命名 / 复制标题 / 复制全部输出 / 关闭 / 关闭其它 / 关闭已结束 / 新窗口打开）
 * - 底部一行：新建终端下拉 + 全部关闭
 *
 * ⚠️ 关于拖拽排序：
 * `terminalStore` 目前**没有** `reorderTabs(ids)` 方法（接口清单里没有），
 * 按任务书的降级要求，这里不实现拖拽排序；标签顺序等于打开顺序。
 */

import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";

import { usePresetStore } from "@/stores/presets";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type { TerminalTab } from "@/types";
import { writeText } from "@/utils/clipboard";
import Icon from "@/components/common/Icon.vue";
import ContextMenu from "@/components/common/ContextMenu.vue";
import BaseInput from "@/components/common/BaseInput.vue";
import { getBridge } from "./bridge";
import NewTerminalMenu from "./NewTerminalMenu.vue";

const terminalStore = useTerminalStore();
const presetStore = usePresetStore();
const uiStore = useUiStore();

// ============================================================
// 状态
// ============================================================

/** 正在行内重命名的标签 */
const editingId = ref("");
/** 重命名草稿 */
const editingTitle = ref("");
/** 重命名输入框 */
const renameInputRef = ref<InstanceType<typeof BaseInput> | null>(null);
/** 正在滚动的标签容器 */
const stripRef = ref<HTMLElement | null>(null);
/** 右键菜单 */
const menuOpen = ref(false);
const menuX = ref(0);
const menuY = ref(0);
const menuTargetId = ref("");

/** 右键菜单项结构（对齐 common/ContextMenu） */
interface MenuItem {
  key: string;
  label: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  divider?: boolean;
  shortcut?: string;
}

const tabs = computed<TerminalTab[]>(() => terminalStore.tabs.value);
const activeId = computed<string>(() => terminalStore.activeId.value);

const menuItems = computed<MenuItem[]>(() => {
  const hasOthers = tabs.value.length > 1;
  return [
    { key: "rename", label: "重命名…", icon: "edit" },
    { key: "copyTitle", label: "复制标题", icon: "copy" },
    { key: "copyAll", label: "复制全部输出", icon: "copy" },
    { key: "div1", label: "", divider: true },
    { key: "close", label: "关闭", icon: "x", danger: true },
    {
      key: "closeOthers",
      label: "关闭其它",
      icon: "trash",
      disabled: !hasOthers,
    },
    { key: "closeFinished", label: "关闭已结束", icon: "inbox" },
    { key: "div2", label: "", divider: true },
  ];
});

// ============================================================
// 滚动
// ============================================================

function scrollStrip(dir: -1 | 1): void {
  const el = stripRef.value;
  if (!el) return;
  el.scrollBy({ left: dir * 240, behavior: "smooth" });
}

/** 激活标签变化时把它滚进可视区 */
function ensureVisible(id: string): void {
  const el = stripRef.value;
  if (!el) return;
  const node = el.querySelector<HTMLElement>(`[data-tab-id="${id}"]`);
  if (!node) return;
  const left = node.offsetLeft;
  const right = left + node.offsetWidth;
  if (left < el.scrollLeft) el.scrollTo({ left: left - 8, behavior: "smooth" });
  else if (right > el.scrollLeft + el.clientWidth) {
    el.scrollTo({ left: right - el.clientWidth + 8, behavior: "smooth" });
  }
}

// ============================================================
// 交互
// ============================================================

function activate(tab: TerminalTab): void {
  if (editingId.value === tab.sessionId) return;
  terminalStore.setActive(tab.sessionId);
  // 让内部终端实例重新 fit（display 切换后尺寸才有效）
  void nextTick(() => {
    ensureVisible(tab.sessionId);
    getBridge(tab.sessionId)?.fit();
  });
}

function closeTab(tab: TerminalTab, e: Event): void {
  e.stopPropagation();
  void terminalStore.close(tab.sessionId);
}

/** 开始行内重命名 */
function startRename(tab: TerminalTab): void {
  editingId.value = tab.sessionId;
  editingTitle.value = tab.title || tab.presetName || "终端";
  void nextTick(() => renameInputRef.value?.focus());
}

/** 提交重命名 */
function commitRename(): void {
  const tab = terminalStore.byId(editingId.value);
  const next = editingTitle.value.trim();
  if (tab && next) tab.title = next;
  editingId.value = "";
}

/** 打开右键菜单 */
function openTabMenu(e: MouseEvent, tab: TerminalTab): void {
  e.preventDefault();
  e.stopPropagation();
  menuTargetId.value = tab.sessionId;
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuOpen.value = true;
}

/** 右键菜单动作 */
async function onMenuSelect(key: string): Promise<void> {
  const id = menuTargetId.value;
  const tab = terminalStore.byId(id);
  if (!tab) return;

  switch (key) {
    case "rename":
      startRename(tab);
      break;
    case "copyTitle": {
      const ok = await writeText(tab.title || tab.presetName || "");
      uiStore.toast(ok ? "success" : "error", ok ? "已复制标题" : "复制失败");
      break;
    }
    case "copyAll": {
      const ok = await writeText(terminalStore.getBuffer(id));
      uiStore.toast(
        ok ? "success" : "error",
        ok ? "已复制全部输出" : "复制失败",
        ok ? "" : "该会话没有输出内容",
      );
      break;
    }
    case "close":
      await terminalStore.close(id);
      break;
    case "closeOthers":
      await terminalStore.closeOthers(id);
      break;
    case "closeFinished":
      await terminalStore.closeFinished();
      break;
    default:
      break;
  }
}

// ============================================================
// 全局动作
// ============================================================

/** 关闭全部标签（危险操作，二次确认） */
async function closeAll(): Promise<void> {
  if (tabs.value.length === 0) return;
  const running = terminalStore.runningCount.value;
  const ok = await uiStore.confirm({
    title: "关闭全部终端",
    message: `确定要关闭全部 ${tabs.value.length} 个标签页吗？`,
    detail:
      running > 0 ? `其中 ${running} 个仍在运行，关闭会强制终止对应进程。` : "",
    danger: true,
    confirmText: "全部关闭",
  });
  if (ok) await terminalStore.closeAll();
}

/** 用当前选中的预设新建一个标签 */
async function runSelectedPreset(): Promise<void> {
  const p = presetStore.byId(presetStore.selectedId.value);
  if (!p) {
    uiStore.toast(
      "warning",
      "请先在左侧选中一条预设",
      "选中后这里可以直接运行它",
    );
    return;
  }
  if (p.placeholderArgs.length > 0) {
    uiStore.openDialog("runParams", p);
    return;
  }
  await terminalStore.openPreset(p, {}, "manual");
}

// ============================================================
// 生命周期
// ============================================================

function onGlobalKey(ev: KeyboardEvent): void {
  // Alt+← / Alt+→ 切换标签
  if (ev.altKey && (ev.key === "ArrowLeft" || ev.key === "ArrowRight")) {
    const list = tabs.value;
    if (list.length < 2) return;
    ev.preventDefault();
    const idx = list.findIndex((t) => t.sessionId === activeId.value);
    const next = ev.key === "ArrowLeft" ? idx - 1 : idx + 1;
    const target = list[(next + list.length) % list.length];
    if (target) activate(target);
  }
  // Ctrl+W 关闭当前标签
  if (
    ev.ctrlKey &&
    !ev.shiftKey &&
    ev.key.toLowerCase() === "w" &&
    activeId.value
  ) {
    ev.preventDefault();
    void terminalStore.close(activeId.value);
  }
}

onMounted(() => {
  window.addEventListener("keydown", onGlobalKey);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKey);
});

/** 当前预设名称（用于新建菜单里显示「用当前预设新建」） */
const currentPresetName = computed<string>(() => {
  const p = presetStore.byId(presetStore.selectedId.value);
  return p ? p.name : "";
});
</script>

<template>
  <div class="cd-tabs">
    <!-- 滚动按钮 -->
    <button
      class="cd-tabs__scroll"
      type="button"
      title="向左滚动标签"
      @click="scrollStrip(-1)"
    >
      <span aria-hidden="true">◀</span>
    </button>

    <!-- 标签条主体 -->
    <div ref="stripRef" class="cd-tabs__strip" @contextmenu.prevent>
      <div
        v-for="tab in tabs"
        :key="tab.sessionId"
        class="cd-tab"
        :class="{
          'is-active': tab.sessionId === activeId,
          'is-off': tab.status !== 'running',
        }"
        :data-tab-id="tab.sessionId"
        :title="`${tab.title || tab.presetName || '终端'} — ${
          tab.status === 'running'
            ? '运行中'
            : tab.status === 'killed'
              ? '已终止'
              : '已结束'
        }`"
        @click="activate(tab)"
        @dblclick="startRename(tab)"
        @contextmenu="openTabMenu($event, tab)"
      >
        <!-- 状态圆点 -->
        <span
          class="cd-tab__dot"
          :class="`is-${tab.status}`"
          aria-hidden="true"
        />

        <Icon name="terminal" :size="13" class="cd-tab__icon" />

        <!-- 行内重命名 -->
        <BaseInput
          v-if="editingId === tab.sessionId"
          ref="renameInputRef"
          v-model="editingTitle"
          size="sm"
          class="cd-tab__rename"
          @click.stop
          @enter="commitRename"
          @blur="commitRename"
          @keydown.esc.stop.prevent="editingId = ''"
        />
        <span v-else class="cd-tab__title">{{
          tab.title || tab.presetName || "终端"
        }}</span>

        <button
          class="cd-tab__close"
          type="button"
          :title="tab.status === 'running' ? '停止并关闭' : '关闭'"
          @click="closeTab(tab, $event)"
        >
          <Icon name="x" :size="11" />
        </button>
      </div>
    </div>

    <button
      class="cd-tabs__scroll"
      type="button"
      title="向右滚动标签"
      @click="scrollStrip(1)"
    >
      <span aria-hidden="true">▶</span>
    </button>

    <!-- 底部操作行 -->
    <div class="cd-tabs__bar">
      <NewTerminalMenu
        :preset-name="currentPresetName"
        @run-preset="runSelectedPreset"
      />

      <span class="cd-tabs__stat">
        <template v-if="terminalStore.runningCount.value > 0">
          <span class="cd-tabs__dot" />
          {{ terminalStore.runningCount.value }} 个运行中
        </template>
        <template v-else>共 {{ tabs.length }} 个标签</template>
      </span>

      <button
        class="cd-tabs__close-all"
        type="button"
        title="关闭全部终端标签（Ctrl+W 关闭当前）"
        :disabled="tabs.length === 0"
        @click="closeAll"
      >
        <Icon name="x" :size="12" /> 全部关闭
      </button>
    </div>

    <!-- 标签右键菜单 -->
    <ContextMenu
      v-model="menuOpen"
      :x="menuX"
      :y="menuY"
      :items="menuItems"
      @select="onMenuSelect"
    />
  </div>
</template>

<style scoped>
.cd-tabs {
  display: flex;
  flex-direction: column;
  flex: none;
  background: var(--bg-panel, #161b22);
  border-bottom: 1px solid var(--border, #262c36);
}

/* ---------- 标签条 ---------- */
.cd-tabs__strip {
  display: flex;
  align-items: stretch;
  gap: 2px;
  height: var(--tabbar-h, 36px);
  padding: 4px 2px 0;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: thin;
  scrollbar-color: var(--scrollbar-thumb, #30363d) transparent;
}

.cd-tabs__strip::-webkit-scrollbar {
  height: 4px;
}
.cd-tabs__strip::-webkit-scrollbar-thumb {
  background: var(--scrollbar-thumb, #30363d);
  border-radius: var(--radius-pill, 999px);
}

.cd-tabs__scroll {
  flex: none;
  width: 20px;
  border: none;
  background: transparent;
  color: var(--text-muted, #6e7b8a);
  font-size: 9px;
  cursor: pointer;
  padding: 0;
  transition:
    color var(--transition-fast, 120ms),
    background var(--transition-fast, 120ms);
}

.cd-tabs__scroll:hover {
  color: var(--text-primary, #e6edf3);
  background: var(--bg-hover, #21262d);
}

/* ---------- 单个标签 ---------- */
.cd-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
  max-width: 220px;
  height: calc(var(--tabbar-h, 36px) - 4px);
  padding: 0 6px 0 8px;
  border: 1px solid var(--border, #262c36);
  border-bottom: none;
  border-radius: var(--radius-md, 4px) var(--radius-md, 4px) 0 0;
  background: var(--bg-app, #0d1117);
  color: var(--text-secondary, #a6b3c2);
  font-size: var(--font-size-sm, 12px);
  cursor: pointer;
  user-select: none;
  transition:
    background var(--transition-fast, 120ms),
    color var(--transition-fast, 120ms),
    border-color var(--transition-fast, 120ms);
}

.cd-tab:hover {
  background: var(--bg-hover, #21262d);
  color: var(--text-primary, #e6edf3);
}

.cd-tab.is-active {
  background: var(--bg-elevated, #1c2129);
  border-color: var(--border-accent, #2f6f6b);
  color: var(--text-primary, #e6edf3);
  box-shadow: inset 0 2px 0 0 var(--accent, #2dd4bf);
}

.cd-tab.is-off {
  opacity: 0.75;
}

.cd-tab__icon {
  flex: none;
  color: var(--text-muted, #6e7b8a);
}

.cd-tab.is-active .cd-tab__icon {
  color: var(--accent, #2dd4bf);
}

.cd-tab__title {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cd-tab__rename {
  flex: 1 1 auto;
  min-width: 80px;
  max-width: 150px;
}

/* 状态圆点 */
.cd-tab__dot {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted, #6e7b8a);
}

.cd-tab__dot.is-running {
  background: var(--info, #58a6ff);
  animation: cd-tab-breathe 1.6s ease-in-out infinite;
}

.cd-tab__dot.is-killed {
  background: var(--warning, #d29922);
}

@keyframes cd-tab-breathe {
  0%,
  100% {
    opacity: 1;
    box-shadow: 0 0 0 0 var(--info-soft, rgba(88, 166, 255, 0.14));
  }
  50% {
    opacity: 0.45;
    box-shadow: 0 0 0 4px transparent;
  }
}

@media (prefers-reduced-motion: reduce) {
  .cd-tab__dot.is-running {
    animation: none;
  }
}

/* 关闭按钮：hover 时才出现 */
.cd-tab__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-muted, #6e7b8a);
  cursor: pointer;
  padding: 0;
  opacity: 0;
  transition:
    opacity var(--transition-fast, 120ms),
    background var(--transition-fast, 120ms);
}

.cd-tab:hover .cd-tab__close,
.cd-tab.is-active .cd-tab__close {
  opacity: 1;
}

.cd-tab__close:hover {
  background: var(--danger-soft, rgba(248, 81, 73, 0.14));
  color: var(--danger, #f85149);
}

/* ---------- 底部操作行 ---------- */
.cd-tabs__bar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 6px;
  border-top: 1px solid var(--border, #262c36);
  background: var(--bg-panel, #161b22);
}

.cd-tabs__stat {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--text-muted, #6e7b8a);
  font-size: var(--font-size-xs, 11px);
}

.cd-tabs__dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--info, #58a6ff);
}

.cd-tabs__close-all {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 20px;
  padding: 0 8px;
  border: 1px solid var(--border, #262c36);
  border-radius: var(--radius-sm, 3px);
  background: var(--bg-elevated, #1c2129);
  color: var(--text-secondary, #a6b3c2);
  font-size: var(--font-size-xs, 11px);
  cursor: pointer;
  transition:
    background var(--transition-fast, 120ms),
    color var(--transition-fast, 120ms);
}

.cd-tabs__close-all:hover:not(:disabled) {
  background: var(--danger-soft, rgba(248, 81, 73, 0.14));
  border-color: var(--danger, #f85149);
  color: var(--danger, #f85149);
}

.cd-tabs__close-all:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
