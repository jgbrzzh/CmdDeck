/**
 * 多标签终端仓库（模块级单例）。
 *
 * 设计要点：
 *
 * 1. **store 不碰 DOM**。后端 PTY 输出先落到 `tab.outputBuffer`，
 *    再由 TerminalView（xterm 实例的持有者）在 `watch` 里 `term.write()`。
 *    store 只通过 `setFit(id, fn)` 拿到一个回调，在数据变化后触发一次重排。
 *
 * 2. **建 tab 的时机**：
 *    - 前端主动打开的会话（openPreset / openShell / openRaw）：
 *      invoke 返回 `TerminalInfo` 后立刻建 tab，所以顺序不会乱。
 *    - 后端主动 spawn 的会话（定时任务 / 工作流 / 批量）：
 *      前端拿不到返回值，只能靠 `onPtyOpen` 事件补建。
 *
 * 3. **缓冲上限**：单个 tab 1MB，超出时淘汰旧输出，保留最新内容，
 *    防止跑几小时的高频输出把内存吃光。
 */

import { computed, ref, type ComputedRef, type Ref } from "vue";

import { terminalApi, toFriendlyError } from "@/api";
import {
  onPtyData,
  onPtyExit,
  onPtyOpen,
  onScheduleFired,
  onWorkflowUpdate,
} from "@/api/events";
import type {
  PtyDataPayload,
  PtyExitPayload,
  PtyOpenPayload,
} from "@/api/events";
import type {
  Preset,
  RunSource,
  SpawnOptions,
  TerminalInfo,
  TerminalTab,
} from "@/types";
import { useUiStore } from "./ui";

/** 单个标签页输出缓冲上限：1MB 字符 */
const MAX_BUFFER = 1024 * 1024;

export interface TerminalStore {
  // ---- 状态 ----
  tabs: Ref<TerminalTab[]>;
  activeId: Ref<string>;
  runningCount: ComputedRef<number>;
  hasTabs: ComputedRef<boolean>;
  activeTab: ComputedRef<TerminalTab | undefined>;

  byId(id: string): TerminalTab | undefined;
  getBuffer(id: string): string;

  /** 注册后端事件监听；返回取消监听的函数 */
  init(): Promise<() => void>;

  // ---- 打开会话 ----
  /** 运行一条预设，返回 sessionId */
  openPreset(
    preset: Preset,
    args: Record<string, string>,
    source?: RunSource,
  ): Promise<string>;
  /** 打开指定类型的交互式 Shell，返回 sessionId */
  openShell(kind: string): Promise<string>;
  /** 通用 spawn，返回 sessionId */
  openRaw(options: SpawnOptions): Promise<string>;
  /** 打开一个已有会话（从历史记录恢复现场），返回 sessionId */
  attach(info: TerminalInfo, snapshot: string): Promise<string>;

  // ---- 交互 ----
  write(id: string, data: string): void;
  resize(id: string, cols: number, rows: number): void;
  stop(id: string): Promise<void>;
  close(id: string): Promise<void>;
  closeAll(): Promise<void>;
  closeOthers(id: string): Promise<void>;
  closeFinished(): Promise<void>;

  // ---- 视图同步 ----
  setActive(id: string): void;
  /** TerminalView 注册自己，保证多个标签的刷新顺序不错乱 */
  setFit(id: string, fn: (() => void) | null): void;
  fitAll(): void;
}

// ============================================================
// 模块级状态
// ============================================================

const tabs = ref<TerminalTab[]>([]);
const activeId = ref("");

/** TerminalView 注册的 fit 回调表 */
const fitCallbacks = new Map<string, () => void>();

/** 已初始化标记，避免重复注册后端监听 */
let initialized = false;

/** 累计的 fit 调度句柄，把同一帧内的多次刷新合并成一次 */
let fitRaf = 0;

// ============================================================
// 派生
// ============================================================

/** 正在运行的会话数 */
const runningCount = computed<number>(
  () => tabs.value.filter((t) => t.status === "running").length,
);

/** 是否存在任何标签页 */
const hasTabs = computed<boolean>(() => tabs.value.length > 0);

/** 当前激活的标签页 */
const activeTab = computed<TerminalTab | undefined>(() => byId(activeId.value));

// ============================================================
// 内部工具
// ============================================================

/** 查找标签页 */
function byId(id: string): TerminalTab | undefined {
  if (!id) return undefined;
  return tabs.value.find((t) => t.sessionId === id);
}

/** 取某个标签页的完整输出缓冲（"复制全部"用） */
function getBuffer(id: string): string {
  return byId(id)?.outputBuffer ?? "";
}

/** 把一条 TerminalInfo 包成 TerminalTab */
function toTab(
  info: TerminalInfo,
  snapshot: string | null = null,
): TerminalTab {
  return {
    ...info,
    pendingSnapshot: snapshot,
    outputBuffer: "",
    outputOffset: 0,
    stoppedByUser: false,
    background: false,
  };
}

/**
 * 向输出缓冲追加数据，并限制总长度。
 *
 * 超出 1MB 时淘汰半个缓冲；绝对偏移保证缓存截断后仍能追加最新输出。
 */
function appendOutput(id: string, data: string): void {
  const tab = byId(id);
  if (!tab || !data) return;

  let buf = tab.outputBuffer + data;
  if (buf.length > MAX_BUFFER) {
    // 一次淘汰半个缓冲，避免达到上限后每一小块输出都复制 1MB。
    let cut = buf.length - Math.floor(MAX_BUFFER / 2);
    const c = buf.charCodeAt(cut);
    if (c >= 0xdc00 && c <= 0xdfff) cut++;
    tab.outputOffset += cut;
    buf = buf.slice(cut);
  }

  tab.outputBuffer = buf;
  // 已经渲染过的快照不再重复写入
  tab.pendingSnapshot = null;
}

/** 往某个标签页追加一行内部提示文字 */
function appendSystemLine(id: string, line: string): void {
  appendOutput(id, `\r\n${line}\r\n`);
}

/**
 * 把刷新合并到下一帧。
 *
 * PTY 高频输出时如果每块数据都立刻触发 fit，xterm 会重复重排导致闪烁；
 * 用 requestAnimationFrame 合并后每帧最多重排一次。
 */
function scheduleFit(id: string): void {
  const fn = fitCallbacks.get(id);
  if (!fn) return;

  if (typeof requestAnimationFrame !== "function") {
    fn();
    return;
  }

  if (fitRaf) return;
  fitRaf = requestAnimationFrame(() => {
    fitRaf = 0;
    fitCallbacks.get(id)?.();
  });
}

/** 新建 tab 并设为激活项 */
function pushTab(tab: TerminalTab): TerminalTab {
  tabs.value = [...tabs.value, tab];
  activeId.value = tab.sessionId;
  return tab;
}

/** 从 tabs 移除（后端 close 失败也要移除，否则会卡住界面） */
function removeTab(id: string): void {
  fitCallbacks.delete(id);
  tabs.value = tabs.value.filter((t) => t.sessionId !== id);
  if (activeId.value === id) {
    // 激活项被删 → 落到相邻的一个
    activeId.value =
      tabs.value.length > 0 ? tabs.value[tabs.value.length - 1].sessionId : "";
  }
}

/** 生成一个 tab 的显示标题（优先用后端给的 title / presetName） */
function tabTitle(info: TerminalInfo): string {
  return info.title || info.presetName || info.kind || "终端";
}

// ============================================================
// 事件监听
// ============================================================

/**
 * 注册全部后端事件监听。
 *
 * @returns 取消监听的函数（在 App 的 `onUnmounted` 或测试里调用）
 */
async function init(): Promise<() => void> {
  const ui = useUiStore();
  const unlisteners: Array<() => void> = [];
  const registrations: Promise<unknown>[] = [];

  const push = (p: Promise<() => void>) => {
    registrations.push(p.then((un) => unlisteners.push(un)));
  };

  // ---- PTY 输出 ----
  push(
    onPtyData((p: PtyDataPayload) => {
      if (!byId(p.sessionId)) return; // 会话已被关闭，忽略迟到数据
      appendOutput(p.sessionId, p.data);
      scheduleFit(p.sessionId);
    }),
  );

  // ---- PTY 退出 ----
  push(
    onPtyExit((p: PtyExitPayload) => {
      const tab = byId(p.sessionId);
      if (!tab) return;

      tab.status = p.status === "killed" ? "killed" : "exited";
      tab.exitCode = p.exitCode;

      const code = p.exitCode === null ? "未知" : String(p.exitCode);
      const text = tab.stoppedByUser
        ? "[进程已被手动终止]"
        : p.exitCode === 0
          ? `[进程已退出，退出码 ${code}：成功]`
          : `[进程已退出，退出码 ${code}：失败]`;
      appendSystemLine(p.sessionId, text);

      ui.toast(
        p.exitCode === 0 || tab.stoppedByUser || p.status === "killed"
          ? "info"
          : "warning",
        tab.stoppedByUser ? "任务已终止" : "任务已结束",
        `${tabTitle(tab)} · ${text.replace(/[[\]]/g, "")}`,
      );

      scheduleFit(p.sessionId);
    }),
  );

  // ---- 后端主动创建的会话（定时任务 / 工作流 / 批量）----
  push(
    onPtyOpen((p: PtyOpenPayload) => {
      // 前端主动开的会话已经在 openRaw 里建过 tab，这里不重复建
      if (byId(p.sessionId)) return;

      const tab = pushTab(toTab(p));
      ui.setView("terminals");
      // 定时任务触发的会话自动切到对应标签页
      scheduleFit(tab.sessionId);
    }),
  );

  // ---- 定时任务触发 ----
  push(
    onScheduleFired((p) => {
      ui.toast(
        "info",
        "定时任务已触发",
        `${p.name}（${p.presetName || "未命名预设"}）正在运行`,
      );
      // 对应会话的标签页已经由 onPtyOpen 建好，这里切过去
      if (byId(p.sessionId)) {
        ui.setView("terminals");
        setActive(p.sessionId);
      }
    }),
  );

  // ---- 工作流状态更新 ----
  push(
    onWorkflowUpdate((run) => {
      if (run.status === "running") {
        ui.toast(
          "info",
          "工作流已开始",
          `${run.workflowName} 正在执行，共 ${run.steps.length} 个步骤`,
        );
        return;
      }
      const ok = run.status === "success";
      ui.toast(
        ok ? "success" : run.status === "canceled" ? "warning" : "error",
        ok
          ? "工作流已完成"
          : run.status === "canceled"
            ? "工作流已取消"
            : "工作流执行失败",
        `${run.workflowName} · ${run.steps.length} 个步骤`,
      );
      // 打开工作流面板展示本次运行结果
      ui.openDialog("workflowEditor", { run });
    }),
  );

  await Promise.all(registrations);
  initialized = true;

  // 一次性清理函数
  return () => {
    unlisteners.forEach((un) => {
      try {
        un();
      } catch (e) {
        console.warn("[CmdDeck] 取消事件监听失败", e);
      }
    });
    unlisteners.length = 0;
    initialized = false;
  };
}

// ============================================================
// 打开会话
// ============================================================

/**
 * 运行一条预设。
 *
 * @param preset 预设对象
 * @param args   占位符参数
 * @param source 运行来源（手动 / 批量 / 定时 / 工作流 / 快速启动 / 快捷键）
 * @returns 新建会话的 sessionId
 */
async function openPreset(
  preset: Preset,
  args: Record<string, string>,
  source: RunSource = "manual",
): Promise<string> {
  try {
    const info = await terminalApi.runPreset(preset.id, args ?? {}, source);
    const tab = pushTab(toTab(info));
    scheduleFit(tab.sessionId);
    return info.sessionId;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 运行预设失败", e);
    useUiStore().toast("error", "运行失败", e.message, 8000);
    throw e;
  }
}

/** 打开一个交互式 Shell（powershell / cmd / pwsh） */
async function openShell(kind: string): Promise<string> {
  try {
    const info = await terminalApi.openShell(kind);
    const tab = pushTab(toTab(info));
    useUiStore().setView("terminals");
    scheduleFit(tab.sessionId);
    return info.sessionId;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 打开 Shell 失败", e);
    useUiStore().toast("error", "打开终端失败", e.message, 8000);
    throw e;
  }
}

/** 通用 spawn：批量、工作流、以及"临时终端"都走这里 */
async function openRaw(options: SpawnOptions): Promise<string> {
  try {
    const info = await terminalApi.spawn(options);
    const tab = pushTab(toTab(info));
    scheduleFit(tab.sessionId);
    return info.sessionId;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 创建会话失败", e);
    useUiStore().toast("error", "创建会话失败", e.message, 8000);
    throw e;
  }
}

/**
 * 打开一个已有会话（从历史记录恢复现场）。
 *
 * 前端不能凭空造一个后端不存在的会话，所以这里要求调用方
 * 先用 `terminalApi.list()` 拿到还活着的 `TerminalInfo`，
 * 再用 `terminalApi.snapshot()` 取回输出快照。
 *
 * @param info     会话信息
 * @param snapshot 已渲染的输出文本，TerminalView 会先写这段再接增量
 */
async function attach(info: TerminalInfo, snapshot: string): Promise<string> {
  // 已经打开过同一个会话，直接聚焦
  const existing = byId(info.sessionId);
  if (existing) {
    setActive(existing.sessionId);
    return existing.sessionId;
  }

  const tab = pushTab(toTab(info, snapshot ?? ""));
  useUiStore().setView("terminals");
  scheduleFit(tab.sessionId);
  return info.sessionId;
}

// ============================================================
// 交互
// ============================================================

/** 往会话写入用户输入 */
function write(id: string, data: string): void {
  const tab = byId(id);
  if (!tab || tab.status !== "running") return;
  terminalApi.write(id, data).catch((err) => {
    console.warn("[CmdDeck] 写入终端失败", toFriendlyError(err));
  });
}

/** 通知后端窗口尺寸变化（PTY 需要知道才能正确换行） */
function resize(id: string, cols: number, rows: number): void {
  if (!byId(id)) return;
  if (
    !Number.isFinite(cols) ||
    !Number.isFinite(rows) ||
    cols <= 0 ||
    rows <= 0
  )
    return;
  const tab = byId(id)!;
  cols = Math.floor(cols);
  rows = Math.floor(rows);
  if (tab.cols === cols && tab.rows === rows) return;
  tab.cols = cols;
  tab.rows = rows;
  terminalApi.resize(id, cols, rows).catch((err) => {
    tab.cols = 0;
    tab.rows = 0;
    // resize 调用非常频繁，失败只记日志
    console.debug("[CmdDeck] resize 失败", toFriendlyError(err));
  });
}

/** 终止进程（保留标签页，可继续查看输出） */
async function stop(id: string): Promise<void> {
  const tab = byId(id);
  if (!tab) return;
  tab.stoppedByUser = true;
  try {
    await terminalApi.kill(id);
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 终止进程失败", e);
    useUiStore().toast("error", "终止进程失败", e.message);
  }
}

/** 关闭一个标签页：先杀进程再从列表移除 */
async function close(id: string): Promise<void> {
  const tab = byId(id);
  if (!tab) return;
  try {
    // 已经在跑的会话要通知后端回收 PTY；已退出的可能已经回收，失败可忽略
    if (tab.status === "running") {
      tab.stoppedByUser = true;
      await terminalApi.kill(id).catch(() => undefined);
    }
    await terminalApi.close(id).catch(() => undefined);
  } finally {
    removeTab(id);
  }
}

/** 关闭全部标签页 */
async function closeAll(): Promise<void> {
  const ids = tabs.value.map((t) => t.sessionId);
  // 先在 UI 上一次性移除，避免逐个 await 时界面卡顿
  fitCallbacks.clear();
  tabs.value = [];
  activeId.value = "";
  await Promise.all(
    ids.map(async (id) => {
      try {
        await terminalApi.kill(id).catch(() => undefined);
        await terminalApi.close(id).catch(() => undefined);
      } catch (e) {
        console.warn("[CmdDeck] 关闭会话失败", e);
      }
    }),
  );
}

/** 只保留当前标签，关闭其它所有标签 */
async function closeOthers(id: string): Promise<void> {
  const others = tabs.value.filter((t) => t.sessionId !== id);
  await Promise.all(others.map((t) => close(t.sessionId)));
  setActive(id);
}

/** 关闭所有已结束的标签（status !== 'running'） */
async function closeFinished(): Promise<void> {
  const finished = tabs.value.filter((t) => t.status !== "running");
  if (finished.length === 0) return;
  await Promise.all(finished.map((t) => close(t.sessionId)));
}

// ============================================================
// 视图同步
// ============================================================

/** 设置当前激活的标签页；被激活的 tab 取消 background 标记 */
function setActive(id: string): void {
  if (!byId(id)) return;
  activeId.value = id;
  tabs.value = tabs.value.map((t) =>
    t.sessionId === id ? { ...t, background: false } : t,
  );
}

/**
 * TerminalView 在 onMounted 注册自己的 fit 回调，onUnmounted 时传 `null` 注销。
 *
 * 注册顺序保证同一时刻多个标签同时收到数据时，界面刷新顺序与事件到达顺序一致。
 */
function setFit(id: string, fn: (() => void) | null): void {
  if (!id) return;
  if (fn === null) fitCallbacks.delete(id);
  else fitCallbacks.set(id, fn);
}

/** 立即重排所有已注册的 xterm（窗口缩放、侧栏折叠后调用） */
function fitAll(): void {
  fitCallbacks.forEach((fn) => {
    try {
      fn();
    } catch (e) {
      console.warn("[CmdDeck] fit 回调异常", e);
    }
  });
}

// ============================================================
// 导出
// ============================================================

const store: TerminalStore = {
  tabs,
  activeId,
  runningCount,
  hasTabs,
  activeTab,

  byId,
  getBuffer,

  init,

  openPreset,
  openShell,
  openRaw,
  attach,

  write,
  resize,
  stop,
  close,
  closeAll,
  closeOthers,
  closeFinished,

  setActive,
  setFit,
  fitAll,
};

export function useTerminalStore(): TerminalStore {
  return store;
}

/** 是否已经调用过 init()（调试用） */
export function isTerminalStoreInitialized(): boolean {
  return initialized;
}
