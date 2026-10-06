/**
 * 剪贴板工具。
 *
 * Tauri 的 WebView2 里 `navigator.clipboard` 在非安全上下文（`tauri://`）
 * 可能不可用，所以保留 `document.execCommand('copy')` 兜底。
 */

/** HTML 转义，用于 textarea 兜底方案 */
function escapeForTextarea(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/**
 * 复制文本到剪贴板。
 *
 * @param text 要复制的纯文本
 * @returns 是否复制成功
 */
export async function writeText(text: string): Promise<boolean> {
  const content = text ?? "";
  if (!content) return false;

  // 方案一：标准剪贴板 API
  try {
    if (navigator?.clipboard?.writeText) {
      await navigator.clipboard.writeText(content);
      return true;
    }
  } catch {
    // 权限被拒或非安全上下文，继续走兜底方案
  }

  // 方案二：隐藏 textarea + execCommand
  return legacyCopy(content);
}

/**
 * 旧版复制方案：临时 textarea 选中后执行 copy 命令。
 * 复制完成（或失败）后立即移除临时节点。
 */
function legacyCopy(content: string): boolean {
  if (typeof document === "undefined") return false;

  const ta = document.createElement("textarea");
  ta.value = content;
  ta.setAttribute("readonly", "");
  ta.setAttribute("aria-hidden", "true");
  // 放到屏幕外，避免复制时页面滚动跳动
  ta.style.position = "fixed";
  ta.style.top = "-9999px";
  ta.style.left = "-9999px";
  ta.style.opacity = "0";

  document.body.appendChild(ta);
  try {
    ta.select();
    ta.setSelectionRange(0, ta.value.length);
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    document.body.removeChild(ta);
  }
}

/**
 * 读取剪贴板文本。浏览器策略可能拒绝读取，失败时返回空串。
 */
export async function readText(): Promise<string> {
  try {
    if (navigator?.clipboard?.readText) {
      return await navigator.clipboard.readText();
    }
  } catch {
    // 忽略：多数环境下不允许读剪贴板
  }
  return "";
}

/**
 * 在系统默认编辑器中打开一个包含 `content` 的临时文件路径。
 * 「导出终端输出」这类操作由后端完成，这里只负责把路径交给 `shell` 插件。
 */
export function isClipboardAvailable(): boolean {
  return typeof navigator !== "undefined" && !!navigator.clipboard;
}

/** 把一段纯文本转成 HTML 预览片段（保留换行与转义） */
export function toHtmlPreview(content: string): string {
  return escapeForTextarea(content)
    .replace(/\r\n/g, "\n")
    .replace(/\n/g, "<br />");
}
