/**
 * 全局键盘快捷键。
 *
 * 注册在 **window 的 keydown** 上，`onMounted` 订阅、`onUnmounted` 退订。
 *
 * 两条重要规则：
 * 1. **输入框豁免**：焦点在 input / textarea / contenteditable / xterm
 *    隐藏输入框里时，单键快捷键（Esc、Ctrl+L 等）全部放行，
 *    否则用户打字打到一半就被劫持。
 * 2. **带修饰键的快捷键不豁免**：Ctrl+K / Ctrl+B 这类在输入框里也需要生效
 *    （用户可以一键跳到快速启动），只有纯单键才让位。
 */

import { onMounted, onBeforeUnmount } from "vue";

import { isEditableTarget } from "@/utils/dom";

/** 快捷键处理器集合，全部可选 */
export interface ShortcutHandlers {
  /** Ctrl+K —— 打开快速启动 */
  quickLaunch?: () => void;
  /** Ctrl+N —— 新建终端 */
  newTerminal?: () => void;
  /** Ctrl+B —— 折叠 / 展开侧栏 */
  toggleSidebar?: () => void;
  /** Ctrl+, —— 打开设置 */
  settings?: () => void;
  /** Ctrl+Shift+P —— 命令面板式快速启动 */
  palette?: () => void;
  /** Ctrl+W —— 关闭当前终端标签 */
  closeTab?: () => void;
  /** Ctrl+L —— 清屏（调用当前 xterm 的 clear） */
  clearScreen?: () => void;
  /** Esc —— 关闭最上层浮层 */
  escape?: () => void;
}

/** 是否是当前环境下的"mac 式 meta"（本项目只跑 Windows，留作兼容） */
function hasMeta(e: KeyboardEvent): boolean {
  return e.ctrlKey || e.metaKey;
}

/** 去掉修饰键后的主键名，统一小写 */
function keyOf(e: KeyboardEvent): string {
  return (e.key || "").toLowerCase();
}

/**
 * 注册全局快捷键。
 *
 * ```ts
 * useGlobalShortcuts({
 *   quickLaunch: () => (ui.quickLaunchOpen = true),
 *   newTerminal: () => terminalStore.openShell(settings.settings.defaultShell),
 *   toggleSidebar: () => ui.toggleSidebar(),
 * })
 * ```
 *
 * 传进来的对象会被记下来，事件触发时再取 —— 这样组件里后续
 * 新增/替换 handler 也能立即生效，不用重新注册。
 */
export function useGlobalShortcuts(handlers: ShortcutHandlers): void {
  let registered = false;

  function onKeyDown(e: KeyboardEvent): void {
    // 输入框里用户正在打字：所有单键快捷键让位
    const editable = isEditableTarget(e.target);
    const key = keyOf(e);
    const withCtrl = hasMeta(e);
    const withShift = e.shiftKey;
    const withAlt = e.altKey;

    // ---------- 组合键（任何位置都生效）----------
    if (withCtrl && withShift && key === "p") {
      handlers.palette?.();
      e.preventDefault();
      return;
    }

    if (withCtrl && !withShift && !withAlt) {
      switch (key) {
        case "k":
          handlers.quickLaunch?.();
          e.preventDefault();
          return;
        case "n":
          handlers.newTerminal?.();
          e.preventDefault();
          return;
        case "b":
          handlers.toggleSidebar?.();
          e.preventDefault();
          return;
        case ",":
          handlers.settings?.();
          e.preventDefault();
          return;
        case "w":
          handlers.closeTab?.();
          e.preventDefault();
          return;
        case "l":
          // Ctrl+L 在浏览器里是"聚焦地址栏"，桌面端没有地址栏，
          // 这里用作清屏；输入框里不劫持（那是文本全选的既有语义）
          if (!editable) {
            handlers.clearScreen?.();
            e.preventDefault();
          }
          return;
        default:
          break;
      }
    }

    // ---------- 单键（输入框里让位）----------
    if (!withCtrl && !withAlt && !withShift) {
      if (key === "escape") {
        handlers.escape?.();
        // Esc 在输入框里不拦截默认行为（清空 xterm 的选中态等）
        if (!editable) e.preventDefault();
      }
    }
  }

  onMounted(() => {
    if (registered) return;
    window.addEventListener("keydown", onKeyDown);
    registered = true;
  });

  onBeforeUnmount(() => {
    if (!registered) return;
    window.removeEventListener("keydown", onKeyDown);
    registered = false;
  });
}
