/**
 * DOM 小工具。
 *
 * 只放「纯 DOM 判断/测量」这类没有依赖的函数，方便 store 与 composable 复用。
 */

/**
 * 判断事件目标是否是可编辑元素。
 *
 * 全局单键快捷键（Esc / Ctrl+L 等）在输入框聚焦时必须放行，
 * 否则用户打字打到一半就被劫持。
 *
 * @param target 事件目标
 */
export function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || !el.tagName) return false;

  const tag = el.tagName.toUpperCase();
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;

  // contenteditable 元素（含其子节点）
  if (el.isContentEditable) return true;

  // xterm 的隐藏输入框
  if (el.classList?.contains("xterm-helper-textarea")) return true;

  return false;
}

/** 是否存在任何已打开的浮层（模态框、快速启动等） */
export function hasOpenOverlay(): boolean {
  return document.querySelectorAll(".overlay-root").length > 0;
}

/** 读取 CSS 变量当前值（用于把颜色变量喂给 xterm 等第三方库） */
export function cssVar(name: string, fallback = ""): string {
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}

/** 让元素可编程聚焦 */
export function focusElement(el: HTMLElement | null | undefined): void {
  if (!el) return;
  try {
    el.focus({ preventScroll: true });
  } catch {
    el.focus();
  }
}

/** 平滑滚动到容器顶部 */
export function scrollTop(
  el: HTMLElement | null | undefined,
  smooth = false,
): void {
  if (!el) return;
  el.scrollTo({ top: 0, behavior: smooth ? "smooth" : "auto" });
}
