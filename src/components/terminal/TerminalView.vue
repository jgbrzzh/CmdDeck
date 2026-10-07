<script setup lang="ts">
/**
 * 单个终端实例视图 —— xterm.js 的唯一持有者。
 *
 * 职责：
 * 1. 创建 / 销毁 xterm 实例与四个 addon（fit / search / web-links / unicode11）
 * 2. 按帧监听输出绝对偏移，缓存截断后继续增量写入 xterm。
 * 3. 用户输入 → `terminalStore.write()` 发给后端 PTY
 * 4. 尺寸同步：ResizeObserver + window resize → `fitAddon.fit()` → `store.resize()`
 * 5. 键盘快捷键（复制 / 粘贴 / 搜索 / 翻页）、内置搜索条、右键菜单
 * 6. 进程结束后的状态条（退出码 / 已被终止）
 *
 * 关于「为什么用 watch 拉取而不是 store 推送」：
 * store 的 `setFit(id, fn)` 只在"数据已追加到 outputBuffer 之后"触发一次 fit 回调，
 * 并不把数据本身交给视图。如果视图只在 fit 回调里读 `getBuffer(id)`，
 * 一次高频输出就会被写进 xterm 很多次（每帧一次），性能很差。
 * 所以本组件把职责拆开：
 * - 输出监听在 requestAnimationFrame 中合并增量。
 * - 数据回调只处理自动贴底；尺寸同步只由容器大小变化触发。
 */

import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";

import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";

import { useSettingsStore } from "@/stores/settings";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type { TerminalTab } from "@/types";
import { writeText, readText } from "@/utils/clipboard";
import { formatClock } from "@/utils/format";
import Icon from "@/components/common/Icon.vue";
import ContextMenu from "@/components/common/ContextMenu.vue";
import { registerBridge, unregisterBridge } from "./bridge";
import { resolveSearchDecoration, resolveTerminalTheme } from "./themes";

// ============================================================
// Props
// ============================================================

const props = defineProps<{ tab: TerminalTab }>();

const terminalStore = useTerminalStore();
const settingsStore = useSettingsStore();
const uiStore = useUiStore();

// ============================================================
// 常量
// ============================================================

/** 判定为「交互式 Shell 会话」的 kind（这些会话才显示欢迎横幅） */
const SHELL_KINDS = ["shell", "cmd", "powershell", "pwsh"];

/** ANSI 颜色：亮青 / 灰 / 加粗 */
const C_BOLD_CYAN = "\x1b[1;38;5;51m";
const C_DIM = "\x1b[38;5;244m";
const C_RESET = "\x1b[0m";

/** CmdDeck ASCII 横幅（纯 ASCII，任何等宽字体都不会缺字） */
const ASCII_BANNER = [
  " ____                  _  ____          _ ",
  "|  _ \\ ___ _ __  _ __| |/ ___|___  __| | ___  ___ __ _ _ __",
  "| |_) / _ \\ '__|| '__| | |   / _ \\/ _' |/ _ \\/ __/ _` | '_ \\",
  "|  _ <  __/ |   | |  | | |_|  __/ (_| |  __/ (_| (_| | | | |",
  "|_| \\_\\___|_|   |_|  |_|\\___\\___|\\__,_|\\___\\___\\__,_|_| |_|",
].join("\r\n");

// ============================================================
// 响应式状态
// ============================================================

/** xterm 挂载点 */
const hostRef = ref<HTMLDivElement | null>(null);
/** 滚动条容器（用于判断是否贴底） */
const viewportRef = ref<HTMLElement | null>(null);
/** 搜索条显隐 */
const searchOpen = ref(false);
/** 搜索关键词 */
const searchKeyword = ref("");
/** 搜索结果计数 */
const searchStat = ref("无结果");
/** 搜索框 DOM，聚焦用 */
const searchInputRef = ref<HTMLInputElement | null>(null);
/** 右键菜单显隐 */
const menuOpen = ref(false);
const menuX = ref(0);
const menuY = ref(0);
/** 是否正在自动跟随底部 */
const stickBottom = ref(true);
/** 是否已经写下欢迎横幅 */
const bannerWritten = ref(false);

// ============================================================
// 非响应式句柄
// ============================================================

let term: Terminal | null = null;
let fitAddon: FitAddon | null = null;
let searchAddon: SearchAddon | null = null;
let resizeObserver: ResizeObserver | null = null;
let searchResultDisposable: { dispose(): void } | null = null;
let windowResizeHandler: (() => void) | null = null;
/** 已经写进 xterm 的缓冲长度 */
let lastWrittenLen = 0;
let outputRaf = 0;
/** 贴合底部的滚轮监听器 */
let scrollHandler: (() => void) | null = null;

const sessionId = computed<string>(() => props.tab.sessionId);

// ============================================================
// 右键菜单项
// ============================================================

/** 右键菜单数据结构（与 common/ContextMenu 的 props 对齐） */
interface MenuItem {
  key: string;
  label: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  divider?: boolean;
  shortcut?: string;
}

const menuItems = computed<MenuItem[]>(() => [
  { key: "copy", label: "复制", icon: "copy", shortcut: "Ctrl+Shift+C" },
  { key: "paste", label: "粘贴", icon: "upload", shortcut: "Ctrl+V" },
  { key: "selectAll", label: "全选", icon: "check" },
  { key: "search", label: "搜索…", icon: "search", shortcut: "Ctrl+Shift+F" },
  { key: "div1", label: "", divider: true },
  { key: "clear", label: "清屏", icon: "trash" },
  { key: "copyAll", label: "复制全部输出", icon: "copy" },
  { key: "div2", label: "", divider: true },
  { key: "newTab", label: "新建终端", icon: "plus" },
  { key: "close", label: "关闭此标签", icon: "x", danger: true },
]);

// ============================================================
// 退出状态
// ============================================================

/** 进程是否已经结束 */
const finished = computed<boolean>(() => props.tab.status !== "running");

/** 状态条文案 */
const exitText = computed<string>(() => {
  const t = props.tab;
  if (t.status === "killed" || t.stoppedByUser) return "已被终止";
  if (t.status === "exited") {
    if (t.exitCode === null) return "已退出（退出码未知）";
    return t.exitCode === 0 ? "退出码 0 · 成功" : `退出码 ${t.exitCode} · 失败`;
  }
  return "";
});

/** 状态条的语义色 class */
const exitClass = computed<string>(() => {
  const t = props.tab;
  if (t.status === "killed" || t.stoppedByUser) return "is-killed";
  if (t.exitCode === 0) return "is-ok";
  return "is-fail";
});

// ============================================================
// 尺寸与滚动
// ============================================================

/** 容器是否有有效尺寸（隐藏的标签页为 0，不能 fit） */
function hasSize(): boolean {
  const el = hostRef.value;
  if (!el) return false;
  return el.clientWidth > 8 && el.clientHeight > 8;
}

/** 判断视口是否贴底 */
function isAtBottom(): boolean {
  const vp = viewportRef.value;
  if (!vp) return true;
  return vp.scrollHeight - vp.scrollTop - vp.clientHeight <= 6;
}

/** 重排尺寸并把新行列数同步给后端 */
function doFit(): void {
  if (!term || !fitAddon || !hasSize()) return;
  try {
    fitAddon.fit();
    if (term.cols > 0 && term.rows > 0)
      terminalStore.resize(sessionId.value, term.cols, term.rows);
  } catch (err) {
    // 容器在动画/折叠过程中可能瞬间为 0，忽略即可
    console.debug("[CmdDeck] fit 失败", err);
  }
}

/** 供 store 在收到数据后调用：重排 + 贴底 */
function onStoreData(): void {
  if (stickBottom.value) term?.scrollToBottom();
}

/** 绑定到 window 的 resize（与 ResizeObserver 双保险） */
function onWindowResize(): void {
  doFit();
}

// ============================================================
// 输出渲染（拉取式增量）
// ============================================================

/** 把 outputBuffer 中尚未渲染的部分写进 xterm */
function flushPending(): void {
  if (!term) return;
  const buf = props.tab.outputBuffer ?? "";
  const end = props.tab.outputOffset + buf.length;
  if (end <= lastWrittenLen) return;
  const chunk = buf.slice(Math.max(0, lastWrittenLen - props.tab.outputOffset));
  lastWrittenLen = end;
  term.write(chunk, () => {
    if (stickBottom.value) term?.scrollToBottom();
  });
}

/** 重放历史快照（从历史记录恢复现场时用） */
function replaySnapshot(): void {
  const snap = props.tab.pendingSnapshot;
  if (!snap) return;
  term?.write(snap);
  props.tab.pendingSnapshot = null;
  lastWrittenLen =
    props.tab.outputOffset + (props.tab.outputBuffer ?? "").length;
  doFit();
  term?.scrollToBottom();
}

// ============================================================
// 欢迎横幅
// ============================================================

/** 当前会话是否是交互式 Shell（只有 Shell 才写欢迎横幅） */
function isShellSession(): boolean {
  const t = props.tab;
  if (t.presetId) return false;
  if (t.temporary) return false;
  return SHELL_KINDS.includes(String(t.kind).toLowerCase());
}

/** 写一段 CmdDeck 欢迎横幅 */
function writeWelcomeBanner(): void {
  if (!term || bannerWritten.value) return;
  if (!settingsStore.settings.value.showWelcomeBanner) return;
  if (!isShellSession()) return;

  bannerWritten.value = true;
  const s = settingsStore.settings.value;
  term.write(
    `${C_BOLD_CYAN}${ASCII_BANNER}${C_RESET}\r\n` +
      `${C_DIM}  Windows 控制台集中管理中心 · 终端会话已就绪${C_RESET}\r\n` +
      `${C_DIM}  工作目录：${props.tab.cwd || "用户主目录"}　|　快捷键：Ctrl+Shift+F 搜索，Shift+PageUp/PageDown 翻页${C_RESET}\r\n` +
      `${C_DIM}  字号 ${s.fontSize}px · 配色 ${s.colorScheme}${C_RESET}\r\n\r\n`,
  );
}

// ============================================================
// 剪贴板
// ============================================================

/** 复制当前选中内容 */
async function copySelection(): Promise<boolean> {
  if (!term) return false;
  const text = term.getSelection();
  if (!text) return false;
  const ok = await writeText(text);
  uiStore.toast(
    ok ? "success" : "error",
    ok ? "已复制" : "复制失败",
    ok ? "" : "请检查系统剪贴板权限",
  );
  return ok;
}

/** 复制全部输出（优先用 xterm 里的完整缓冲，退化到 store 缓冲） */
async function copyAllOutput(): Promise<boolean> {
  if (!term) return false;
  const text = props.tab.outputBuffer || "";
  if (!text) {
    uiStore.toast("warning", "没有可复制的内容", "该会话尚未产生任何输出");
    return false;
  }
  const ok = await writeText(text);
  uiStore.toast(
    ok ? "success" : "error",
    ok ? "已复制全部输出" : "复制失败",
    ok ? "" : "请检查系统剪贴板权限",
  );
  return ok;
}

/** 粘贴剪贴板内容到终端（走 onData，兼容 readline / cmd） */
async function pasteClipboard(): Promise<void> {
  if (!term) return;
  const text = await readText();
  if (!text) {
    uiStore.toast("warning", "剪贴板为空", "没有可粘贴的文本");
    return;
  }
  // term.paste 会走 onData 通道，和手动输入完全等价
  term.paste(text);
}

// ============================================================
// 搜索
// ============================================================

/** 当前配色方案的搜索高亮配色 */
function currentDecorations() {
  const d = resolveSearchDecoration(settingsStore.settings.value.colorScheme);
  return {
    matchBackground: d.matchBackground,
    matchBorder: d.matchBorder,
    matchOverviewRuler: d.matchBorder,
    activeMatchBackground: d.activeMatchBackground,
    activeMatchBorder: d.activeMatchBorder,
    activeMatchColorOverviewRuler: d.activeMatchBackground,
  };
}

/** 执行一次搜索（向前） */
function findNext(): void {
  if (!searchAddon || !searchKeyword.value) return;
  const found = searchAddon.findNext(searchKeyword.value, {
    caseSensitive: false,
    incremental: true,
    decorations: currentDecorations(),
  });
  searchStat.value = found ? searchStat.value : "无结果";
}

/** 执行一次搜索（向后） */
function findPrevious(): void {
  if (!searchAddon || !searchKeyword.value) return;
  const found = searchAddon.findPrevious(searchKeyword.value, {
    caseSensitive: false,
    decorations: currentDecorations(),
  });
  searchStat.value = found ? searchStat.value : "无结果";
}

/** 打开搜索条并聚焦输入框 */
function openSearch(): void {
  searchOpen.value = true;
  void nextTick(() => {
    searchInputRef.value?.focus();
    searchInputRef.value?.select();
  });
}

/** 关闭搜索条并清理高亮 */
function closeSearch(): void {
  searchOpen.value = false;
  searchKeyword.value = "";
  searchStat.value = "无结果";
  searchAddon?.clearDecorations();
  term?.focus();
}

// ============================================================
// 右键菜单
// ============================================================

function openMenu(e: MouseEvent): void {
  e.preventDefault();
  e.stopPropagation();
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuOpen.value = true;
}

/** 右键菜单项点击 */
function onMenuSelect(key: string): void {
  switch (key) {
    case "copy":
      void copySelection();
      break;
    case "paste":
      void pasteClipboard();
      break;
    case "selectAll":
      term?.selectAll();
      break;
    case "search":
      openSearch();
      break;
    case "clear":
      term?.clear();
      term?.scrollToBottom();
      break;
    case "copyAll":
      void copyAllOutput();
      break;
    case "newTab":
      void terminalStore
        .openShell(settingsStore.settings.value.defaultShell || "powershell")
        .catch(() => undefined);
      break;
    case "close":
      void terminalStore.close(sessionId.value);
      break;
    default:
      break;
  }
}

// ============================================================
// 键盘
// ============================================================

/**
 * xterm 键盘拦截。
 *
 * 返回 `true` 表示"xterm 继续按默认方式处理该按键"，
 * 返回 `false` 表示"已经处理掉了，不要再传给 xterm"。
 */
function onKeyEvent(ev: KeyboardEvent): boolean {
  // Ctrl+Shift+F 打开搜索条
  if (ev.ctrlKey && ev.shiftKey && ev.key.toLowerCase() === "f") {
    openSearch();
    return false;
  }

  // Ctrl+Shift+C 复制选中
  if (ev.ctrlKey && ev.shiftKey && ev.key.toLowerCase() === "c") {
    void copySelection();
    return false;
  }

  // Ctrl+C：有选区就复制，没有选区必须原样送给 PTY（Ctrl+C 中断）
  if (ev.ctrlKey && !ev.shiftKey && ev.key.toLowerCase() === "c") {
    if (term?.hasSelection()) {
      void copySelection();
      return false;
    }
    return true;
  }

  // Ctrl+V 粘贴
  if (ev.ctrlKey && !ev.shiftKey && ev.key.toLowerCase() === "v") {
    void pasteClipboard();
    return false;
  }

  // Shift+PageUp / PageDown 翻页
  if (ev.shiftKey && ev.key === "PageUp") {
    term?.scrollLines(-Math.max(5, (term?.rows ?? 20) - 2));
    stickBottom.value = false;
    return false;
  }
  if (ev.shiftKey && ev.key === "PageDown") {
    term?.scrollLines(Math.max(5, (term?.rows ?? 20) - 2));
    return false;
  }

  return true;
}

// ============================================================
// 生命周期
// ============================================================

onMounted(() => {
  const host = hostRef.value;
  if (!host) return;

  const s = settingsStore.settings.value;

  term = new Terminal({
    allowProposedApi: true,
    convertEol: false,
    cursorBlink: s.cursorBlink,
    cursorStyle: "block",
    cursorWidth: 2,
    disableStdin: false,
    fontFamily: s.fontFamily || "Consolas, monospace",
    fontSize: Math.min(40, Math.max(8, s.fontSize || 14)),
    drawBoldTextInBrightColors: true,
    fastScrollModifier: "alt",
    lineHeight: 1.2,
    letterSpacing: 0,
    macOptionIsMeta: false,
    rightClickSelectsWord: false,
    scrollback: Math.min(200000, Math.max(200, s.scrollback || 5000)),
    scrollOnUserInput: true,
    screenReaderMode: false,
    theme: resolveTerminalTheme(s.colorScheme),
    wordSeparator: " ()[]{}',\"`─-–—？！，。；：、",
    overviewRulerWidth: 12,
  });

  fitAddon = new FitAddon();
  searchAddon = new SearchAddon({ highlightLimit: 2000 });
  const webLinksAddon = new WebLinksAddon();
  const unicodeAddon = new Unicode11Addon();

  term.loadAddon(fitAddon);
  term.loadAddon(searchAddon);
  term.loadAddon(webLinksAddon);
  term.loadAddon(unicodeAddon);

  // Unicode 11 的宽字符表对中文/emoji 的列宽判断更准
  try {
    term.unicode.activeVersion = "11";
  } catch {
    // 部分构建里该项是只读的，忽略即可
  }

  term.open(host);

  viewportRef.value = host.querySelector(
    ".xterm-viewport",
  ) as HTMLElement | null;

  // 用户输入 → 后端
  term.onData((data: string) => terminalStore.write(sessionId.value, data));

  // 键盘拦截
  term.attachCustomKeyEventHandler(onKeyEvent);

  // 搜索结果计数
  searchResultDisposable = searchAddon.onDidChangeResults((ev) => {
    if (!searchOpen.value) return;
    if (ev.resultCount <= 0) searchStat.value = "无结果";
    else searchStat.value = `${ev.resultIndex + 1}/${ev.resultCount}`;
  });

  // 手动滚动时取消"贴底跟随"
  if (viewportRef.value) {
    scrollHandler = () => {
      stickBottom.value = isAtBottom();
    };
    viewportRef.value.addEventListener("scroll", scrollHandler, {
      passive: true,
    });
  }

  // 首次尺寸
  doFit();
  // 历史重放 → 欢迎横幅 → 增量输出
  replaySnapshot();
  writeWelcomeBanner();
  flushPending();

  // 注册 store 的 fit 通道（数据到达后由 store 调用）
  terminalStore.setFit(sessionId.value, onStoreData);

  // 供工具栏 / 标签条通过 bridge 调用
  registerBridge(sessionId.value, {
    clear: () => {
      term?.clear();
      term?.scrollToBottom();
    },
    search: openSearch,
    copyAll: () => {
      void copyAllOutput();
      return true;
    },
    focus: () => term?.focus(),
    fit: doFit,
  });

  // 容器尺寸变化
  if (typeof ResizeObserver !== "undefined") {
    resizeObserver = new ResizeObserver(() => {
      doFit();
      if (stickBottom.value) term?.scrollToBottom();
    });
    resizeObserver.observe(host);
  }

  // 窗口尺寸变化（覆盖 ResizeObserver 覆盖不到的情况，比如 DPI 变化）
  if (typeof window !== "undefined") {
    windowResizeHandler = onWindowResize;
    window.addEventListener("resize", windowResizeHandler);
  }

  // 被激活时才抢焦点，避免多标签同时抢
  if (terminalStore.activeId.value === sessionId.value) term.focus();
});

onBeforeUnmount(() => {
  cancelAnimationFrame(outputRaf);
  terminalStore.setFit(sessionId.value, null);
  unregisterBridge(sessionId.value);

  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  if (windowResizeHandler && typeof window !== "undefined") {
    window.removeEventListener("resize", windowResizeHandler);
    windowResizeHandler = null;
  }
  if (viewportRef.value && scrollHandler) {
    viewportRef.value.removeEventListener("scroll", scrollHandler);
    scrollHandler = null;
  }
  searchResultDisposable?.dispose();
  searchResultDisposable = null;

  // term.dispose() 会一并释放所有 addon
  term?.dispose();
  term = null;
  fitAddon = null;
  searchAddon = null;
});

// ============================================================
// 监听
// ============================================================

/** 输出增量：拉取式，store 追加后触发 */
watch(
  () => props.tab.outputOffset + props.tab.outputBuffer.length,
  () => {
    if (!outputRaf)
      outputRaf = requestAnimationFrame(() => {
        outputRaf = 0;
        flushPending();
      });
  },
);

/** 标签被激活 → 抢焦点 + 重排（display 切换后尺寸才有意义） */
watch(
  () => terminalStore.activeId.value,
  (id) => {
    if (id !== sessionId.value) return;
    void nextTick(() => {
      doFit();
      if (stickBottom.value) term?.scrollToBottom();
      term?.focus();
    });
  },
);

/** 配色方案变化 → 热切换 */
watch(
  () => settingsStore.settings.value.colorScheme,
  (scheme) => {
    if (term) term.options.theme = resolveTerminalTheme(scheme);
  },
);

/** 字号 / 字体变化 → 热应用 */
watch(
  () =>
    [
      settingsStore.settings.value.fontSize,
      settingsStore.settings.value.fontFamily,
    ] as const,
  ([size, family]) => {
    if (!term) return;
    term.options.fontSize = Math.min(40, Math.max(8, size || 14));
    term.options.fontFamily = family || "Consolas, monospace";
    doFit();
  },
);

/** 光标闪烁 / 回滚行数变化 */
watch(
  () =>
    [
      settingsStore.settings.value.cursorBlink,
      settingsStore.settings.value.scrollback,
    ] as const,
  ([blink, back]) => {
    if (!term) return;
    term.options.cursorBlink = blink;
    term.options.scrollback = Math.min(200000, Math.max(200, back || 5000));
  },
);

// ============================================================
// 对外动作
// ============================================================

/** 状态条上的「关闭」 */
function closeTab(): void {
  void terminalStore.close(sessionId.value);
}

/** 状态条上的「清屏」 */
function clearScreen(): void {
  term?.clear();
  term?.scrollToBottom();
}

/** 给外部（标签条右键菜单）用的运行时长文案 */
function elapsed(): string {
  return formatClock(Math.max(0, Date.now() - props.tab.startedAt));
}

// 暴露给父组件的少量能力（父组件用 ref 调用）
defineExpose({
  openSearch,
  clearScreen,
  closeTab,
  elapsed,
  termReady: computed(() => term !== null),
});
</script>

<template>
  <div class="cd-term" :class="{ 'is-finished': finished }">
    <!-- xterm 挂载点 -->
    <div ref="hostRef" class="cd-term__host" @contextmenu="openMenu" />

    <!-- 搜索条 -->
    <div v-if="searchOpen" class="cd-term__search" @click.stop>
      <Icon name="search" :size="14" class="cd-term__search-icon" />
      <input
        ref="searchInputRef"
        v-model="searchKeyword"
        class="cd-term__search-input"
        type="text"
        placeholder="在输出中查找…"
        title="在当前终端输出中查找（回车查找下一个，Shift+回车查找上一个）"
        @keydown.enter.exact.prevent="findNext"
        @keydown.enter.shift.prevent="findPrevious"
        @keydown.esc.prevent="closeSearch"
      />
      <span class="cd-term__search-stat">{{ searchStat }}</span>
      <button
        class="cd-term__search-btn"
        type="button"
        title="上一个（Shift+回车）"
        aria-label="查找上一个"
        @click="findPrevious"
      >
        <span class="cd-term__search-arrow">▲</span>
      </button>
      <button
        class="cd-term__search-btn"
        type="button"
        title="下一个（回车）"
        aria-label="查找下一个"
        @click="findNext"
      >
        <span class="cd-term__search-arrow">▼</span>
      </button>
      <button
        class="cd-term__search-btn"
        type="button"
        title="关闭搜索（Esc）"
        @click="closeSearch"
      >
        <Icon name="x" :size="13" />
      </button>
    </div>

    <!-- 退出状态条 -->
    <div v-if="finished" class="cd-term__exit" :class="exitClass">
      <Icon :name="exitClass === 'is-ok' ? 'check' : 'flame'" :size="14" />
      <span class="cd-term__exit-text">{{ exitText }}</span>
      <span class="cd-term__exit-meta">
        {{ tab.kind || "shell" }} · 已运行 {{ elapsed() }}
      </span>
      <span class="cd-term__exit-actions">
        <button
          class="cd-term__exit-btn"
          type="button"
          title="清空当前屏幕"
          @click="clearScreen"
        >
          <Icon name="trash" :size="13" /> 清屏
        </button>
        <button
          class="cd-term__exit-btn"
          type="button"
          title="关闭这个标签页"
          @click="closeTab"
        >
          <Icon name="x" :size="13" /> 关闭
        </button>
      </span>
    </div>

    <!-- 右键菜单 -->
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
/* ============================================================
   容器
   ============================================================ */
.cd-term {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  background: var(--bg-app, #0d1117);
  overflow: hidden;
}

.cd-term.is-finished {
  /* 结束后整体降一点饱和度，视觉上能和"运行中"区分开 */
  box-shadow: inset 0 0 0 1px var(--border, #262c36);
}

.cd-term__host {
  flex: 1 1 auto;
  width: 100%;
  min-height: 0;
  overflow: hidden;
}

/* ============================================================
   搜索条
   ============================================================ */
.cd-term__search {
  position: absolute;
  top: 8px;
  right: 12px;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 4px 0 8px;
  background: var(--bg-elevated, #1c2129);
  border: 1px solid var(--border-strong, #3a424e);
  border-radius: var(--radius-md, 4px);
  box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.45));
}

.cd-term__search-icon {
  color: var(--text-muted, #6e7b8a);
  flex: none;
}

.cd-term__search-input {
  width: 180px;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary, #e6edf3);
  font-family: var(--font-ui, sans-serif);
  font-size: var(--font-size-sm, 12px);
}

.cd-term__search-input::placeholder {
  color: var(--text-muted, #6e7b8a);
}

.cd-term__search-stat {
  min-width: 46px;
  text-align: center;
  color: var(--text-muted, #6e7b8a);
  font-size: var(--font-size-xs, 11px);
  font-family: var(--font-mono, Consolas, monospace);
  user-select: none;
}

.cd-term__search-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--radius-sm, 3px);
  background: transparent;
  color: var(--text-secondary, #a6b3c2);
  cursor: pointer;
  padding: 0;
  transition:
    background var(--transition-fast, 120ms),
    color var(--transition-fast, 120ms);
}

.cd-term__search-btn:hover {
  background: var(--bg-hover, #21262d);
  color: var(--text-primary, #e6edf3);
}

.cd-term__search-arrow {
  font-size: 10px;
  line-height: 1;
}

/* ============================================================
   退出状态条
   ============================================================ */
.cd-term__exit {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 28px;
  padding: 0 10px;
  border-top: 1px solid var(--border, #262c36);
  background: var(--bg-panel, #161b22);
  font-size: var(--font-size-xs, 11px);
  color: var(--text-secondary, #a6b3c2);
}

.cd-term__exit.is-ok {
  border-top-color: var(--success, #3fb950);
  background: var(--success-soft, rgba(63, 185, 80, 0.14));
  color: var(--success, #3fb950);
}

.cd-term__exit.is-fail {
  border-top-color: var(--danger, #f85149);
  background: var(--danger-soft, rgba(248, 81, 73, 0.14));
  color: var(--danger, #f85149);
}

.cd-term__exit.is-killed {
  border-top-color: var(--warning, #d29922);
  background: var(--warning-soft, rgba(210, 153, 34, 0.14));
  color: var(--warning, #d29922);
}

.cd-term__exit-text {
  font-family: var(--font-mono, Consolas, monospace);
  font-weight: var(--font-weight-medium, 500);
}

.cd-term__exit-meta {
  color: var(--text-muted, #6e7b8a);
  margin-left: 4px;
}

.cd-term__exit-actions {
  margin-left: auto;
  display: inline-flex;
  gap: 4px;
}

.cd-term__exit-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 20px;
  padding: 0 8px;
  border: 1px solid var(--border, #262c36);
  border-radius: var(--radius-sm, 3px);
  background: var(--bg-elevated, #1c2129);
  color: inherit;
  font-family: var(--font-ui, sans-serif);
  font-size: var(--font-size-xs, 11px);
  cursor: pointer;
  transition:
    background var(--transition-fast, 120ms),
    border-color var(--transition-fast, 120ms);
}

.cd-term__exit-btn:hover {
  background: var(--bg-hover, #21262d);
  border-color: var(--border-strong, #3a424e);
}
</style>
