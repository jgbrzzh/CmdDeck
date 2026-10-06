<script setup lang="ts">
/**
 * 「新建终端」下拉菜单。
 *
 * 内容：
 * - 系统探测到的 Shell 列表（`systemApi.shells()`）：中文名 + 路径 tooltip，不可用置灰
 * - 快捷项：用当前选中的预设新建 / 打开 CMD / 打开 PowerShell
 *
 * 组件自己拉一次 Shell 列表并缓存；下拉面板点外部自动收起。
 */

import { onBeforeUnmount, onMounted, ref } from "vue";

import { systemApi, toFriendlyError } from "@/api";
import { useSettingsStore } from "@/stores/settings";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type { ShellOption } from "@/types";
import Icon from "@/components/common/Icon.vue";

const props = withDefaults(
  defineProps<{
    /** 当前选中的预设名（决定「用当前预设新建」是否可用） */
    presetName?: string;
  }>(),
  { presetName: "" },
);

const emit = defineEmits<{ (e: "runPreset"): void }>();

const terminalStore = useTerminalStore();
const settingsStore = useSettingsStore();
const uiStore = useUiStore();

// ============================================================
// 状态
// ============================================================

const open = ref(false);
const loading = ref(false);
const shells = ref<ShellOption[]>([]);
const rootRef = ref<HTMLElement | null>(null);

// ============================================================
// 数据
// ============================================================

/** 拉取系统里可用的 Shell（失败时保留空列表并提示一次） */
async function loadShells(force = false): Promise<void> {
  if (!force && shells.value.length > 0) return;
  loading.value = true;
  try {
    shells.value = await systemApi.shells();
  } catch (err) {
    const e = toFriendlyError(err);
    console.warn("[CmdDeck] 探测 Shell 失败", e);
    shells.value = [];
    uiStore.toast("error", "探测 Shell 失败", e.message);
  } finally {
    loading.value = false;
  }
}

// ============================================================
// 交互
// ============================================================

function toggle(): void {
  open.value = !open.value;
  if (open.value) void loadShells();
}

/** 按 shell.id 打开一个交互式终端 */
async function openShellById(shell: ShellOption): Promise<void> {
  if (!shell.available) return;
  close();
  try {
    // shell.id 可能带参数（例如 pwsh 带 -NoLogo），后端 open_shell 只认 kind
    const kind = shell.id.replace(/[^a-z0-9]/gi, "") || shell.id;
    await terminalStore.openShell(kind);
  } catch (err) {
    const e = toFriendlyError(err);
    uiStore.toast("error", "打开终端失败", e.message);
  }
}

/** 用快捷方式打开常见 Shell */
async function openCommon(kind: string, label: string): Promise<void> {
  close();
  try {
    await terminalStore.openShell(kind);
  } catch (err) {
    const e = toFriendlyError(err);
    uiStore.toast("error", `打开${label}失败`, e.message);
  }
}

function close(): void {
  open.value = false;
}

/** 「用当前预设新建」——抛给父组件去弹参数框 */
function runCurrentPreset(): void {
  close();
  if (!props.presetName) {
    uiStore.toast("warning", "没有选中的预设", "请先在左侧列表里选中一条预设");
    return;
  }
  emit("runPreset");
}

// ============================================================
// 生命周期
// ============================================================

function onDocClick(e: MouseEvent): void {
  if (!open.value) return;
  const el = rootRef.value;
  if (el && !el.contains(e.target as Node)) close();
}

onMounted(() => {
  document.addEventListener("mousedown", onDocClick);
  // 预热一次，用户点开时就不用等
  void loadShells();
});

onBeforeUnmount(() => {
  document.removeEventListener("mousedown", onDocClick);
});
</script>

<template>
  <div ref="rootRef" class="cd-newterm">
    <button
      class="cd-newterm__trigger"
      type="button"
      title="新建终端（可选择 Shell 类型）"
      @click="toggle"
    >
      <Icon name="plus" :size="13" />
      <span>新建终端</span>
      <span class="cd-newterm__caret" aria-hidden="true">▾</span>
    </button>

    <div v-if="open" class="cd-newterm__panel" @click.stop>
      <p class="cd-newterm__section">可用 Shell</p>

      <p v-if="loading" class="cd-newterm__empty">正在探测系统 Shell…</p>
      <p v-else-if="shells.length === 0" class="cd-newterm__empty">
        未探测到可用的 Shell，可直接用「打开 PowerShell」。
      </p>

      <button
        v-for="shell in shells"
        :key="shell.id"
        class="cd-newterm__item"
        type="button"
        :disabled="!shell.available"
        :title="
          shell.available
            ? shell.path
            : `${shell.label} 未安装（${shell.path || '未找到路径'}）`
        "
        @click="openShellById(shell)"
      >
        <Icon name="terminal" :size="14" class="cd-newterm__item-icon" />
        <span class="cd-newterm__item-label">
          {{ shell.label }}
          <small>{{ shell.path || "未检测到路径" }}</small>
        </span>
        <span v-if="!shell.available" class="cd-newterm__badge">未安装</span>
      </button>

      <div class="cd-newterm__divider" />

      <p class="cd-newterm__section">快捷操作</p>

      <button
        class="cd-newterm__item"
        type="button"
        :disabled="!presetName"
        :title="
          presetName
            ? `用「${presetName}」新建终端`
            : '请先在左侧列表里选中一条预设'
        "
        @click="runCurrentPreset"
      >
        <Icon name="zap" :size="14" class="cd-newterm__item-icon" />
        <span class="cd-newterm__item-label">
          用当前预设新建
          <small>{{ presetName || "未选中预设" }}</small>
        </span>
      </button>

      <button
        class="cd-newterm__item"
        type="button"
        title="打开传统 CMD 命令行"
        @click="openCommon('cmd', 'CMD')"
      >
        <Icon name="code" :size="14" class="cd-newterm__item-icon" />
        <span class="cd-newterm__item-label">
          打开 CMD
          <small>cmd.exe</small>
        </span>
      </button>

      <button
        class="cd-newterm__item"
        type="button"
        title="打开 Windows PowerShell"
        @click="
          openCommon(
            settingsStore.settings.value.defaultShell || 'powershell',
            'PowerShell',
          )
        "
      >
        <Icon name="terminal" :size="14" class="cd-newterm__item-icon" />
        <span class="cd-newterm__item-label">
          打开 PowerShell
          <small>powershell.exe</small>
        </span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.cd-newterm {
  position: relative;
  display: inline-flex;
}

.cd-newterm__trigger {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
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

.cd-newterm__trigger:hover {
  background: var(--bg-hover, #21262d);
  color: var(--text-primary, #e6edf3);
}

.cd-newterm__caret {
  font-size: 8px;
  color: var(--text-muted, #6e7b8a);
}

/* ---------- 面板 ---------- */
.cd-newterm__panel {
  position: absolute;
  left: 0;
  top: calc(100% + 4px);
  z-index: var(--z-context-menu, 800);
  width: 268px;
  padding: 5px;
  background: var(--bg-elevated, #1c2129);
  border: 1px solid var(--border-strong, #3a424e);
  border-radius: var(--radius-md, 4px);
  box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.45));
  animation: cd-newterm-in 160ms ease-out;
}

@keyframes cd-newterm-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.cd-newterm__section {
  margin: 2px 4px 4px;
  color: var(--text-muted, #6e7b8a);
  font-size: var(--font-size-xs, 11px);
  letter-spacing: 0.04em;
}

.cd-newterm__empty {
  margin: 2px 4px 6px;
  color: var(--text-muted, #6e7b8a);
  font-size: var(--font-size-xs, 11px);
  line-height: 1.5;
}

.cd-newterm__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 6px;
  border: none;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-primary, #e6edf3);
  font-size: var(--font-size-sm, 12px);
  text-align: left;
  cursor: pointer;
  transition: background var(--transition-fast, 120ms);
}

.cd-newterm__item:hover:not(:disabled) {
  background: var(--bg-hover, #21262d);
}

.cd-newterm__item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.cd-newterm__item-icon {
  flex: none;
  color: var(--accent, #2dd4bf);
}

.cd-newterm__item-label {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  line-height: 1.35;
}

.cd-newterm__item-label small {
  color: var(--text-muted, #6e7b8a);
  font-size: 10px;
  font-family: var(--font-mono, Consolas, monospace);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cd-newterm__badge {
  flex: none;
  padding: 1px 5px;
  border: 1px solid var(--border, #262c36);
  border-radius: var(--radius-pill, 999px);
  color: var(--text-muted, #6e7b8a);
  font-size: 10px;
}

.cd-newterm__divider {
  height: 1px;
  margin: 5px 2px;
  background: var(--border, #262c36);
}
</style>
