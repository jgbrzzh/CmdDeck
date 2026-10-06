/**
 * 终端桥接通道（模块级注册表）。
 *
 * 为什么需要它：
 * `TerminalToolbar.vue`（清屏 / 复制全部 / 搜索 / 停止）和 `TerminalTabs.vue`
 * （右键菜单里的清屏、复制全部）都**持有 xterm 实例之外的 DOM**，
 * 但真正的操作对象是 `TerminalView.vue` 内部那个 `Terminal`。
 *
 * store 的 `setFit(id, fn)` 只提供一个"重排"回调，语义太窄不适合复用，
 * 所以这里另开一条私有通道：
 *
 * - `TerminalView` 在 `onMounted` 调 `registerBridge(sessionId, api)`，
 *   在 `onBeforeUnmount` 调 `unregisterBridge(sessionId)`；
 * - `TerminalToolbar` / `TerminalTabs` 用 `getBridge(sessionId)` 拿到 api 后调用。
 *
 * 这层**不进 store**，避免为了两个 UI 按钮把 xterm 相关类型泄漏到全局状态里。
 */

/** 外部可对某个终端实例做的操作 */
export interface TerminalBridgeApi {
  /** 清屏（保留换行，滚动到底部） */
  clear(): void;
  /** 打开并聚焦搜索框 */
  search(): void;
  /** 复制全部输出，返回是否复制成功 */
  copyAll(): boolean;
  /** 聚焦终端（切换标签后调用） */
  focus(): void;
  /** 强制重排一次（窗口/侧栏尺寸变化时调用） */
  fit(): void;
}

/** sessionId → 操作句柄 */
const bridges = new Map<string, TerminalBridgeApi>();

/**
 * 注册某个会话的操作句柄。
 *
 * @param id  会话 ID（`tab.sessionId`）
 * @param api 操作实现
 */
export function registerBridge(id: string, api: TerminalBridgeApi): void {
  if (!id) return;
  bridges.set(id, api);
}

/** 注销某个会话的操作句柄（组件卸载时必须调用，否则会泄漏 DOM 引用） */
export function unregisterBridge(id: string): void {
  if (!id) return;
  bridges.delete(id);
}

/**
 * 取某个会话的操作句柄。
 *
 * @param id 会话 ID
 * @returns 未注册（视图还没挂载或已卸载）时返回 `undefined`，调用方需判空
 */
export function getBridge(id: string): TerminalBridgeApi | undefined {
  if (!id) return undefined;
  return bridges.get(id);
}

/** 是否已有该会话的操作句柄（用于禁用工具栏按钮） */
export function hasBridge(id: string): boolean {
  return !!id && bridges.has(id);
}

/** 清空全部注册（切换数据目录 / 退出时兜底调用） */
export function clearAllBridges(): void {
  bridges.clear();
}
