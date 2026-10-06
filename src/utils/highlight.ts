/**
 * 命令文本高亮辅助函数。
 *
 * 预设列表里会把命令文本渲染成 HTML（配合 `v-html`），把 `{{key}}` 用
 * `<mark>` 标出来，让用户一眼看到「这条命令需要哪些参数」。
 *
 * ⚠️ 安全约定：`v-html` 会绕过 Vue 的模板转义，这里**必须**先把
 * `< > & " '` 全转义掉，否则预设名称里一个 `<script>` 就能注入。
 */

/** 捕获组：单个占位符键名 */
const PLACEHOLDER_GROUP = "[\\w\\u4e00-\\u9fa5\\-\\.]+";

/** HTML 实体转义表 */
const ENTITIES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

/**
 * HTML 转义。用于任何需要拼接进 `v-html` 的用户数据。
 *
 * @param text 原始文本
 * @returns 转义后的安全 HTML 片段
 */
export function escapeHtml(text: string): string {
  if (!text) return "";
  return text.replace(/[&<>"']/g, (ch) => ENTITIES[ch] ?? ch);
}

/** 高亮后的额外 class，方便在样式表里调整 `<mark>` 的配色 */
export const HIGHLIGHT_CLASS = "cmd-highlight";

/**
 * 把命令文本转成安全 HTML，其中 `{{key}}` 被 `<mark>` 包裹。
 *
 * @param text 命令文本（未转义的原始字符串）
 * @param placeholderClass `<mark>` 上的 class，默认 `cmd-highlight`
 * @returns 可直接交给 `v-html` 的字符串
 */
export function highlightPlaceholders(
  text: string,
  placeholderClass = HIGHLIGHT_CLASS,
): string {
  if (!text) return "";

  // 关键：先转义再高亮。转义后 {{ }} 仍然保留（它们不是 HTML 特殊字符）
  const safe = escapeHtml(text);
  const re = new RegExp(`\\{\\{\\s*(${PLACEHOLDER_GROUP})\\s*\\}\\}`, "g");
  return safe.replace(re, `<mark class="${placeholderClass}">{{$1}}</mark>`);
}

/**
 * 高亮**多行**命令文本：保留换行（转成 `<br />`），其余同 `highlightPlaceholders`。
 * 用于预设编辑器里的实时命令预览。
 */
export function highlightMultiline(
  text: string,
  placeholderClass = HIGHLIGHT_CLASS,
): string {
  if (!text) return "";
  return highlightPlaceholders(text, placeholderClass)
    .replace(/\r\n/g, "\n")
    .replace(/\n/g, "<br />");
}

/**
 * 把命令文本按「程序 + 参数」拆成多个 `<span>`，参数用弱化色显示。
 *
 * @param program 主程序名
 * @param args    参数数组
 */
export function highlightCommand(program: string, args: string[]): string {
  const head = `<span class="cmd-program">${escapeHtml(program)}</span>`;
  const tail = (args ?? [])
    .map((a) => `<span class="cmd-arg">${highlightPlaceholders(a)}</span>`)
    .join(" ");
  return tail ? `${head} ${tail}` : head;
}

/**
 * 生成关键字高亮后的 HTML，用于终端输出预览 / 审计日志搜索。
 *
 * @param text 原始文本
 * @param keywords 需要标黄的词（大小写不敏感）
 */
export function highlightKeywords(text: string, keywords: string[]): string {
  const safe = escapeHtml(text);
  const words = (keywords ?? [])
    .map((k) => k.trim())
    .filter((k) => k.length > 0);
  if (words.length === 0) return safe;

  // 最长优先，避免短词先匹配吃掉长词
  const pattern = words
    .sort((a, b) => b.length - a.length)
    .map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");

  const re = new RegExp(`(${pattern})`, "gi");
  return safe.replace(re, '<mark class="cmd-keyword">$1</mark>');
}

/** 把纯文本转成一行摘要（压掉换行、限制长度） */
export function toSingleLine(text: string, maxLen = 120): string {
  if (!text) return "";
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > maxLen ? `${flat.slice(0, maxLen - 1)}…` : flat;
}
