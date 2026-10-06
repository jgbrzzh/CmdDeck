/**
 * 界面状态仓库（模块级单例）。
 *
 * 这里只放「跨组件共享的界面状态」：当前视图、弹窗、Toast、确认框、
 * 侧栏折叠、主题解析、右键菜单。业务数据在 presets / terminals / settings 三个 store 里。
 *
 * 主题联动：这里的 `theme` 是**解析后**的 dark/light，
 * `settings.theme` 可能是 system，此时监听 `prefers-color-scheme` 跟随系统。
 */

import { computed, ref, watch, type ComputedRef, type Ref } from "vue";

import { shortId } from "@/utils/debounce";
import { isEditableTarget } from "@/utils/dom";
import type { DialogName, Toast, ViewName } from "@/types";
import { useSettingsStore } from "./settings";

// ============================================================
// 类型
// ============================================================

export interface ConfirmOptions {
  title: string;
  message: string;
  detail?: string;
  danger?: boolean;
  confirmText?: string;
  cancelText?: string;
}

/** 确认框的完整状态：固定字段 + 弹窗时传入的选项 */
export type ConfirmState = { open: boolean } & ConfirmOptions;

/** 右键菜单项 */
export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: string;
  /** 危险项（删除、终止进程…）用红色 */
  danger?: boolean;
  disabled?: boolean;
  /** 分隔线（label 被忽略） */
  divider?: boolean;
}

/** 右键菜单位置与内容 */
export interface ContextMenuState {
  x: number;
  y: number;
  items: ContextMenuItem[];
  /** 触发菜单的预设 id，方便菜单项回调里取用 */
  presetId: string;
}

export interface UiStore {
  // ---- 视图与布局 ----
  view: Ref<ViewName>;
  dialog: Ref<DialogName>;
  dialogPayload: Ref<unknown>;
  quickLaunchOpen: Ref<boolean>;
  sidebarCollapsed: Ref<boolean>;
  /** 解析后的实际主题 */
  theme: Ref<"dark" | "light">;

  // ---- Toast ----
  toasts: Ref<Toast[]>;
  toast(
    type: Toast["type"],
    title: string,
    message?: string,
    duration?: number,
  ): string;
  dismissToast(id: string): void;
  clearToasts(): void;

  // ---- 确认框 ----
  confirmState: Ref<ConfirmState>;
  confirmResolver: ((v: boolean) => void) | null;
  confirm(opts: ConfirmOptions): Promise<boolean>;
  resolveConfirm(value: boolean): void;

  // ---- 导航 ----
  openDialog(name: DialogName, payload?: unknown): void;
  closeDialog(): void;
  setView(v: ViewName): void;
  toggleSidebar(): void;
  setSidebarCollapsed(collapsed: boolean): void;
  toggleTheme(): void;
  setTheme(t: "dark" | "light"): void;

  // ---- 右键菜单 ----
  contextMenu: Ref<ContextMenuState | null>;
  openContextMenu(
    e: MouseEvent,
    items: ContextMenuItem[],
    presetId?: string,
  ): void;
  closeContextMenu(): void;

  // ---- 搜索框聚焦（TopBar 监听 token 自行 focus） ----
  searchFocusToken: Ref<number>;
  requestSearchFocus(): void;

  // ---- 派生 ----
  isOverlayOpen: ComputedRef<boolean>;
}

// ============================================================
// 模块级状态
// ============================================================

const view = ref<ViewName>("presets");
const dialog = ref<DialogName>("none");
const dialogPayload = ref<unknown>(null);
const quickLaunchOpen = ref(false);
const sidebarCollapsed = ref(false);
const theme = ref<"dark" | "light">("dark");

const toasts = ref<Toast[]>([]);

const contextMenu = ref<ContextMenuState | null>(null);
const searchFocusToken = ref(0);

/** 确认框初始状态 */
function emptyConfirm(): ConfirmState {
  return {
    open: false,
    title: "确认操作",
    message: "",
    detail: "",
    danger: false,
    confirmText: "确定",
    cancelText: "取消",
  };
}

const confirmState = ref<ConfirmState>(emptyConfirm());

/** 当前挂起的确认框 resolve 函数（模块级变量，不放进 ref 以免被意外序列化） */
let confirmResolver: ((v: boolean) => void) | null = null;

/** toast 自动关闭定时器表 */
const toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** 同时最多保留的 toast 数量，超出时挤掉最旧的 */
const MAX_TOASTS = 6;

// ============================================================
// Toast
// ============================================================

/**
 * 弹一条右下角提示。
 *
 * @param type     语义类型，决定配色与默认时长
 * @param title    标题（一行）
 * @param message  详情（可多行）
 * @param duration 毫秒；不传时 info/success/warning 为 4000，error 为 8000；传 0 表示不自动关闭
 * @returns toast id，方便手动 dismiss
 */
function toast(
  type: Toast["type"],
  title: string,
  message = "",
  duration?: number,
): string {
  const id = shortId("toast");
  const ms =
    duration === undefined ? (type === "error" ? 8000 : 4000) : duration;

  toasts.value = [...toasts.value, { id, type, title, message, duration: ms }];

  // 超出上限时移除最旧的一条（从数组头部）
  if (toasts.value.length > MAX_TOASTS) {
    const dropped = toasts.value.slice(0, toasts.value.length - MAX_TOASTS);
    dropped.forEach((t) => {
      const timer = toastTimers.get(t.id);
      if (timer) {
        clearTimeout(timer);
        toastTimers.delete(t.id);
      }
    });
    toasts.value = toasts.value.slice(-MAX_TOASTS);
  }

  if (ms > 0) {
    const timer = setTimeout(() => dismissToast(id), ms);
    toastTimers.set(id, timer);
  }

  return id;
}

/** 手动关闭一条 toast */
function dismissToast(id: string): void {
  const timer = toastTimers.get(id);
  if (timer) {
    clearTimeout(timer);
    toastTimers.delete(id);
  }
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

/** 清空全部 toast（切换大视图或退出前用） */
function clearToasts(): void {
  toastTimers.forEach((t) => clearTimeout(t));
  toastTimers.clear();
  toasts.value = [];
}

// ============================================================
// 确认框
// ============================================================

/**
 * 弹出确认框，等待用户点「确定 / 取消」。
 *
 * 重复调用时会把上一个未决的 Promise 用 `false` resolve 掉，
 * 避免调用方的 `await` 永远挂起。
 */
function confirm(opts: ConfirmOptions): Promise<boolean> {
  // 先收尾上一个
  if (confirmResolver) {
    const prev = confirmResolver;
    confirmResolver = null;
    prev(false);
  }

  confirmState.value = {
    open: true,
    title: opts.title ?? "确认操作",
    message: opts.message ?? "",
    detail: opts.detail ?? "",
    danger: opts.danger ?? false,
    confirmText: opts.confirmText ?? "确定",
    cancelText: opts.cancelText ?? "取消",
  };

  return new Promise<boolean>((resolve) => {
    confirmResolver = resolve;
  });
}

/** ConfirmDialog 组件点按钮时调用，结束当前 Promise */
function resolveConfirm(value: boolean): void {
  const r = confirmResolver;
  confirmResolver = null;
  confirmState.value = { ...confirmState.value, open: false };
  if (r) r(value);
}

// ============================================================
// 导航 / 弹窗
// ============================================================

/** 打开一个弹窗；payload 由具体弹窗组件自己断言类型 */
function openDialog(name: DialogName, payload?: unknown): void {
  dialogPayload.value = payload ?? null;
  dialog.value = name;
}

/** 关闭当前弹窗并清空 payload */
function closeDialog(): void {
  dialog.value = "none";
  dialogPayload.value = null;
  closeContextMenu();
}

/** 切换主视图 */
function setView(v: ViewName): void {
  view.value = v;
  // 切换视图时收起右键菜单，避免菜单悬空
  closeContextMenu();
}

/** 折叠 / 展开侧栏 */
function toggleSidebar(): void {
  sidebarCollapsed.value = !sidebarCollapsed.value;
}

/** 显式设置侧栏折叠状态 */
function setSidebarCollapsed(collapsed: boolean): void {
  sidebarCollapsed.value = collapsed;
}

// ============================================================
// 右键菜单
// ============================================================

/** 在鼠标位置弹出右键菜单；贴近屏幕边缘时自动内收 */
function openContextMenu(
  e: MouseEvent,
  items: ContextMenuItem[],
  presetId = "",
): void {
  e.preventDefault();
  e.stopPropagation();

  const margin = 8;
  const estW = 196;
  const estH = items.length * 30 + 12;
  let x = e.clientX;
  let y = e.clientY;

  if (typeof window !== "undefined") {
    if (x + estW + margin > window.innerWidth)
      x = Math.max(margin, window.innerWidth - estW - margin);
    if (y + estH + margin > window.innerHeight)
      y = Math.max(margin, window.innerHeight - estH - margin);
  }

  contextMenu.value = { x, y, items, presetId };
}

/** 关闭右键菜单 */
function closeContextMenu(): void {
  contextMenu.value = null;
}

// ============================================================
// 搜索框聚焦
// ============================================================

/** 请求 TopBar 聚焦搜索框（自增 token，TopBar watch 后自行 focus） */
function requestSearchFocus(): void {
  searchFocusToken.value += 1;
}

// ============================================================
// 主题
// ============================================================

/** 在 dark / light 之间切换（落库） */
async function toggleTheme(): Promise<void> {
  const next = theme.value === "dark" ? "light" : "dark";
  await setThemeImpl(next);
}

/** 直接设置主题（落库） */
async function setThemeImpl(t: "dark" | "light"): Promise<void> {
  await useSettingsStore().setTheme(t);
}

const settingsStore = useSettingsStore();

// 1) settings.theme 变化 → 更新解析主题（立即 applyAll 已经写过 DOM，这里只同步 ref）
watch(
  () => settingsStore.settings.value.theme,
  (t) => {
    if (t === "dark" || t === "light") {
      theme.value = t;
    } else {
      // system：跟随系统偏好
      theme.value =
        typeof window !== "undefined" && window.matchMedia
          ? window.matchMedia("(prefers-color-scheme: dark)").matches
            ? "dark"
            : "light"
          : "dark";
    }
  },
  { immediate: true },
);

// 2) 主题为 system 时，监听系统偏好变化
if (typeof window !== "undefined" && window.matchMedia) {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onSystemChange = () => {
    if (settingsStore.settings.value.theme === "system") {
      theme.value = mq.matches ? "dark" : "light";
    }
  };
  if (mq.addEventListener) mq.addEventListener("change", onSystemChange);
  else if (mq.addListener) mq.addListener(onSystemChange);
}

// 3) 主题 ref 变化 → 确保 DOM 与之同步（外部直接改 ui.theme 也生效）
watch(theme, (t) => {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", t);
  }
});

// ============================================================
// 派生状态
// ============================================================

/** 当前是否有任何浮层打开（模态框 / 快速启动 / 右键菜单） */
const isOverlayOpen = computed(
  () =>
    dialog.value !== "none" ||
    quickLaunchOpen.value ||
    contextMenu.value !== null,
);

// ============================================================
// 全局事件：点击空白处关掉右键菜单；Esc 关闭浮层
// ============================================================

if (typeof document !== "undefined") {
  document.addEventListener("click", () => closeContextMenu());
  document.addEventListener("contextmenu", (e) => {
    // 在输入框 / xterm 里保留系统右键菜单
    if (isEditableTarget(e.target)) return;
  });
}

// ============================================================
// 导出
// ============================================================

const store: UiStore = {
  view,
  dialog,
  dialogPayload,
  quickLaunchOpen,
  sidebarCollapsed,
  theme,

  toasts,
  toast,
  dismissToast,
  clearToasts,

  confirmState,
  get confirmResolver() {
    return confirmResolver;
  },
  set confirmResolver(v: ((v: boolean) => void) | null) {
    confirmResolver = v;
  },
  confirm,
  resolveConfirm,

  openDialog,
  closeDialog,
  setView,
  toggleSidebar,
  setSidebarCollapsed,
  toggleTheme,
  setTheme: setThemeImpl,

  contextMenu,
  openContextMenu,
  closeContextMenu,

  searchFocusToken,
  requestSearchFocus,

  isOverlayOpen,
};

export function useUiStore(): UiStore {
  return store;
}
