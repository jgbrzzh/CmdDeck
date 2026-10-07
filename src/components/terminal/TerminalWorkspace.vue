<script setup lang="ts">
/**
 * 终端工作区 —— 组合标签条、工具栏与全部 xterm 实例。
 *
 * 关键设计：**用 `v-show` 而不是 `v-if` 渲染每个终端视图**。
 * `v-if` 会在切换标签时销毁/重建 `TerminalView`，xterm 实例随之 dispose，
 * 用户会丢失：滚动位置、搜索高亮、输入法状态，PTY 里已输出但还没被读走的数据也可能闪一下。
 * `v-show` 只切 `display`，所有实例常驻，切标签无需重建终端。
 *
 * 无 props / 无 emits。
 */

import { computed, watch } from "vue";
import { useProductivityStore } from "@/stores/productivity";
import { productivityApi } from "@/api/productivity";

import { usePresetStore } from "@/stores/presets";
import { useSettingsStore } from "@/stores/settings";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type { TerminalTab } from "@/types";
import EmptyState from "@/components/common/EmptyState.vue";
import BaseButton from "@/components/common/BaseButton.vue";
import TerminalTabs from "./TerminalTabs.vue";
import TerminalToolbar from "./TerminalToolbar.vue";
import TerminalView from "./TerminalView.vue";

const terminalStore = useTerminalStore();
const presetStore = usePresetStore();
const settingsStore = useSettingsStore();
const uiStore = useUiStore();
const productivity = useProductivityStore();
const secondary = computed(
  () =>
    tabs.value.find(
      (t) =>
        t.sessionId === productivity.secondaryId.value &&
        t.sessionId !== activeId.value,
    )?.sessionId ||
    tabs.value.filter((t) => t.sessionId !== activeId.value).at(-1)
      ?.sessionId ||
    "",
);
const split = computed(
  () => productivity.layoutMode.value !== "single" && !!secondary.value,
);
async function saveLayout() {
  try {
    const layout = {
      mode: productivity.layoutMode.value,
      tabs: tabs.value.map((t) => ({
        title: t.title,
        kind: t.kind,
        cwd: t.cwd,
        presetId: t.presetId,
      })),
      active: tabs.value.findIndex((t) => t.sessionId === activeId.value),
      secondary: tabs.value.findIndex((t) => t.sessionId === secondary.value),
    };
    await productivityApi.layout(
      productivity.config.value.activeWorkspace,
      layout,
    );
    productivity.config.value.layouts[
      productivity.config.value.activeWorkspace
    ] = layout;
    uiStore.toast("success", "终端布局已保存");
  } catch (e) {
    uiStore.toast("error", "布局保存失败", String(e));
  }
}
function changeSplit() {
  window.setTimeout(() => terminalStore.fitAll(), 50);
}
function selectSecondary(event: Event) {
  productivity.secondaryId.value = (event.target as HTMLSelectElement).value;
  changeSplit();
}

/** 全部标签（顺序 = 打开顺序） */
const tabs = computed<TerminalTab[]>(() =>
  terminalStore.tabs.value.filter(
    (t) => (t.workspaceId || "") === productivity.config.value.activeWorkspace,
  ),
);
const activeId = computed<string>(() => terminalStore.activeId.value);
const empty = computed<boolean>(() => tabs.value.length === 0);
watch(() => productivity.layoutMode.value, changeSplit);
watch(() => secondary.value, changeSplit);
watch(
  () => productivity.config.value.activeWorkspace,
  () => {
    const id = productivity.config.value.activeWorkspace,
      l = productivity.config.value.layouts[id];
    if (!tabs.value.length && l && productivity.config.value.restoreLayout) {
      terminalStore.restoreTabs(l.tabs, id);
      productivity.layoutMode.value = l.mode;
      productivity.secondaryId.value = tabs.value[l.secondary]?.sessionId || "";
    }
    terminalStore.setActive(tabs.value[l?.active || 0]?.sessionId || "");
    changeSplit();
  },
  { immediate: true },
);
watch(
  () => terminalStore.activeId.value,
  () => {
    if (
      tabs.value.length &&
      !tabs.value.some((t) => t.sessionId === terminalStore.activeId.value)
    )
      terminalStore.setActive(tabs.value[0].sessionId);
  },
);

// ============================================================
// 空状态动作
// ============================================================

/** 新建一个默认 Shell 终端 */
async function openDefaultShell(): Promise<void> {
  const kind = settingsStore.settings.value.defaultShell || "powershell";
  await terminalStore.openShell(kind);
}

/** 打开运行参数弹窗（没有选中预设时提示） */
function runPreset(): void {
  const p = presetStore.byId(presetStore.selectedId.value);
  if (!p) {
    uiStore.toast(
      "warning",
      "请先选中一条预设",
      "在左侧预设列表中点击一条预设后再运行",
    );
    return;
  }
  uiStore.openDialog("runParams", p);
}
</script>

<template>
  <section class="cd-workspace">
    <!-- 标签条 + 工具栏 -->
    <template v-if="!empty">
      <TerminalTabs />
      <TerminalToolbar />
      <div class="layout-tools">
        <select v-model="productivity.layoutMode.value" aria-label="终端分屏">
          <option value="single">单终端</option>
          <option value="columns">左右分屏</option>
          <option value="rows">上下分屏</option></select
        ><select
          v-if="split"
          :value="secondary"
          @change="selectSecondary"
          aria-label="第二终端"
        >
          <option
            v-for="tab in tabs.filter((t) => t.sessionId !== activeId)"
            :key="tab.sessionId"
            :value="tab.sessionId"
          >
            {{ tab.title }}
          </option></select
        ><button @click="saveLayout">保存布局</button
        ><button
          v-if="terminalStore.activeTab.value?.presetId"
          @click="
            uiStore.openDialog(
              'runParams',
              presetStore.byId(terminalStore.activeTab.value.presetId),
            )
          "
        >
          重新运行预设
        </button>
      </div>
    </template>

    <!-- 终端主体：全部实例常驻，仅切换 display -->
    <div
      class="cd-workspace__body"
      :class="{
        'split-columns': split && productivity.layoutMode.value === 'columns',
        'split-rows': split && productivity.layoutMode.value === 'rows',
      }"
    >
      <div
        v-for="tab in terminalStore.tabs.value"
        v-show="
          (tab.workspaceId || '') ===
            productivity.config.value.activeWorkspace &&
          (tab.sessionId === activeId || (split && tab.sessionId === secondary))
        "
        :key="tab.sessionId"
        class="cd-workspace__pane"
        :class="{
          'pane-primary': tab.sessionId === activeId,
          'pane-secondary': tab.sessionId === secondary,
        }"
        @mousedown="
          tab.sessionId === secondary &&
          ((productivity.secondaryId.value = activeId),
          terminalStore.setActive(tab.sessionId))
        "
      >
        <TerminalView :tab="tab" />
      </div>

      <!-- 空状态 -->
      <div v-if="empty" class="cd-workspace__empty">
        <EmptyState
          icon="terminal"
          title="还没有终端"
          description="打开一个交互式 Shell，或直接运行一条预设指令，输出会实时显示在这里。"
        >
          <template #action>
            <div class="cd-workspace__empty-actions">
              <BaseButton
                variant="primary"
                icon="plus"
                @click="openDefaultShell"
              >
                新建 PowerShell 终端
              </BaseButton>
              <BaseButton variant="default" icon="play" @click="runPreset"
                >运行一条预设</BaseButton
              >
            </div>
          </template>
        </EmptyState>
      </div>
    </div>
  </section>
</template>

<style scoped>
.cd-workspace {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  background: var(--bg-app, #0d1117);
}

.cd-workspace__body {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

/* 每个终端面板绝对定位铺满，切标签只换 z-index/display */
.cd-workspace__pane {
  position: absolute;
  inset: 0;
  min-width: 0;
  min-height: 0;
}

.cd-workspace__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--space-6, 24px);
}

.cd-workspace__empty-actions {
  display: flex;
  gap: var(--space-2, 8px);
  justify-content: center;
  flex-wrap: wrap;
}
.layout-tools {
  display: flex;
  gap: 6px;
  padding: 6px 10px;
  flex-wrap: wrap;
  border-bottom: 1px solid var(--border-default);
}
.split-columns,
.split-rows {
  display: grid;
  gap: 2px;
  background: var(--border-default);
}
.split-columns {
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  grid-template-areas: "primary secondary";
}
.split-rows {
  grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
  grid-template-areas: "primary" "secondary";
}
.split-columns .cd-workspace__pane,
.split-rows .cd-workspace__pane {
  position: relative;
  inset: auto;
  overflow: hidden;
  background: var(--bg-app);
}
.pane-primary {
  grid-area: primary;
}
.pane-secondary {
  grid-area: secondary;
}
</style>
